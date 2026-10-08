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
    interest::{Index, Interest, Rule, TRACK_EVERY, TRACK_REACH},
    net::{self, ACT, Act, CMDS, Check, Cmd, Event, Far, FarWhat, Snap},
    pilot::Summary,
    seats::{self, Drive},
    told::{self, Named},
};
use glam::Vec3;
use lunar_core::{scenario::PlayerDef, structure::schedule::coasted, view::View};
use lunar_net::{Frame, PlayerState, Reader, RigidState, Writer, flag};
use lunar_ship::{
    Pace,
    sync::{self, Digest, Shadow},
};
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
/// Hit points a second a hand may mend, as a share of what the part has, where tests are let be
/// (whatever is in the hands); elsewhere its welder's rate (`gear.jsonc`) and this much more
/// (acts held up on the way that come together). A second's worth may come at once.
const MEND_RATE: f32 = 0.5;
const MEND_SLACK: f32 = 1.5;
/// Of the time a welder takes to put back a part that is gone, what one player must let pass
/// between two.
const REBUILD_SLACK: f64 = 0.8;
/// Steps back the server keeps where each body was (300 ms): what a shot at a person, aimed at
/// them as the shooter's game had them, is judged against (`PLAN_AUTORITATIVO.md` §3.5) the day
/// people can be hurt; and whatever else asks where someone was a moment ago.
pub const REWIND: usize = 18;

/// Where one player's body was at a step (`Host::bodies_at`): its feet and the way up there, its
/// eye, and what it stood in (structure, and the eye in its frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyAt {
    pub id: u32,
    pub feet: glam::DVec3,
    pub up: glam::DVec3,
    pub eye: glam::DVec3,
    pub ride: Option<(u64, Vec3)>,
}

/// Steps a structure changed by a hand is left as it is before what was not worth telling of it
/// (less than `lunar_ship::sync`'s step of hit points) is told.
const SETTLE: u64 = 15;
/// Steps between two things of a kind with no rate of its own let fly by one player (a reload).
const RELOAD: u64 = 12;
/// Bytes of events in one message at most (what a reliable message carries, less its head), and
/// how many (what a player's game reads in one).
const EVENTS_ROOM: usize = lunar_net::MAX_TELL - 32;
const EVENTS_MOST: usize = 4096;
/// What one player may send, by the step: messages of commands (one a step, and a frame that
/// catches up sends several at once), acts (a lever dragged sends one a step) and structures
/// asked for again; each with what may come at once. Past that it is dropped and counted.
const CMDS_RATE: f32 = 2.0;
const CMDS_BURST: f32 = 24.0;
const ACTS_RATE: f32 = 1.5;
const ACTS_BURST: f32 = 120.0;
const RESYNC_RATE: f32 = 8.0 / 60.0;
const RESYNC_BURST: f32 = 16.0;
/// Steps in a row a player's command may be guessed as the last one; past them nothing more is
/// asked of the body (its game is not heard: it does not go on walking, or firing, on its own).
const GUESS_MOST: u32 = 30;

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
    /// Seconds the body of who is cut off waits for them where it is, standing still, to be
    /// taken back by their key (`Act::Back`); 0: it goes with them.
    pub keep: f64,
}

impl Default for HostConfig {
    fn default() -> HostConfig {
        HostConfig { cheats: false, snap_every: 2, snap_room: lunar_net::MAX_HINT - 8, rule: Rule::default(), launches: 30.0, region: 0, keep: 60.0 }
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
    /// Bodies taken back by who had been cut off, and left for good (nobody came back for them).
    pub back: u64,
    pub forgotten: u64,
    /// Events too big for any message (they could not be told).
    pub too_big: u64,
    /// Messages dropped for coming faster than a player may send them (commands, acts,
    /// structures asked for again).
    pub flooded: u64,
}

/// One player, as the server has them besides their body.
struct Peer {
    id: u32,
    /// What takes their body back if they are cut off (`Event::Hello`, `Act::Back`).
    key: u64,
    /// Cut off: their body waits for them until this step (its id is one no connection has).
    lost: Option<u64>,
    /// Their commands by step (`RING` of them, each at `step % RING`).
    cmds: Vec<Cmd>,
    /// The newest step of theirs we have (to step with), the newest heard of at all (late ones
    /// too), and the last command stepped with.
    newest: Option<u64>,
    heard: Option<u64>,
    /// The step of the newest digests told them (`Host::checks`).
    checked: u64,
    /// Not in the world yet and catching up with it, alone: the step the body is at.
    behind: Option<u64>,
    last: Cmd,
    /// Steps in a row their command was guessed.
    guessing: u32,
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
    rested: Vec<u64>,
    /// Events for them, encoded (`net::append_event`), and where each ends in it.
    events: Vec<u8>,
    ends: Vec<usize>,
    /// The step each of them was told at (what it says is as the game was at the end of it), and
    /// the step the game is at as they are told (`Host::stamp`): a message says one step for all
    /// it carries, so what waits to go keeps its own.
    at: Vec<u64>,
    now: u64,
    /// What goes out to them this step: a snapshot, the events.
    quick: Vec<u8>,
    /// What they know only from afar (reused), its word to them this step, and whether the last
    /// one had anything (an empty one is said once, to clear it).
    far: Vec<Far>,
    tracks: Vec<u8>,
    far_told: bool,
    sure: Vec<u8>,
    /// What may be let fly and mended now (refilled each step), and when each kind of thing was
    /// last let fly (step): nothing goes faster than its kind can.
    launches: f32,
    mend: f32,
    fired: Vec<(crate::blasts::What, u64)>,
    /// When they last put back a part that was gone (step).
    rebuilt: Option<u64>,
    /// Told that all there was round them when they came has been told (`Event::Ready`).
    ready: bool,
    /// What may still come of their commands, acts and structures asked for again (refilled each
    /// step, `CMDS_RATE`…).
    cmds_left: f32,
    acts_left: f32,
    resyncs_left: f32,
    /// (reused)
    snap: Snap,
    order: Vec<usize>,
}

impl Peer {
    fn new(id: u32, key: u64) -> Peer {
        Peer {
            id,
            key,
            lost: None,
            cmds: vec![Cmd::default(); RING],
            newest: None,
            heard: None,
            checked: 0,
            behind: None,
            last: Cmd::default(),
            guessing: 0,
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
            rested: Vec::new(),
            events: Vec::new(),
            ends: Vec::new(),
            at: Vec::new(),
            now: 0,
            quick: Vec::new(),
            far: Vec::new(),
            tracks: Vec::new(),
            far_told: false,
            sure: Vec::new(),
            launches: 0.0,
            mend: 0.0,
            fired: Vec::new(),
            rebuilt: None,
            ready: false,
            cmds_left: CMDS_BURST,
            acts_left: ACTS_BURST,
            resyncs_left: RESYNC_BURST,
            snap: Snap::default(),
            order: Vec::new(),
        }
    }

    fn cmd_at(&self, step: u64) -> Option<&Cmd> {
        let c = &self.cmds[(step % RING as u64) as usize];
        (c.step == step && self.newest.is_some_and(|n| n >= step)).then_some(c)
    }

    /// Whether a message of kind `kind` (`CMDS`, `ACT`) comes faster than they may send it (else
    /// it is counted against what may come).
    fn flooded(&mut self, kind: u8) -> bool {
        let left = if kind == CMDS { &mut self.cmds_left } else { &mut self.acts_left };
        if *left < 1.0 {
            return true;
        }
        *left -= 1.0;
        false
    }

    fn tell(&mut self, e: &Event) {
        net::append_event(e, &mut self.events);
        self.ends.push(self.events.len());
        self.at.push(self.now);
    }

    fn tell_bytes(&mut self, encoded: &[u8]) {
        self.events.extend_from_slice(encoded);
        self.ends.push(self.events.len());
        self.at.push(self.now);
    }

    /// Nothing more to tell them.
    fn untold(&mut self) {
        self.events.clear();
        self.ends.clear();
        self.at.clear();
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
    /// a hand (`Event::State`), and when what is left untold of it is to be told (step); by id.
    shadows: Vec<(u64, u64, Shadow, u64)>,
    /// What holds each structure, as last told (by id): what changed is told (`Event::Hold`), and
    /// the systems of the ships that took or let go (their clamps' lists).
    helds: Vec<(u64, Option<lunar_core::structure::hold::Held>)>,
    /// What every player is drawn from this step (theirs left out of their own snapshot).
    states: Vec<(u32, PlayerState)>,
    /// Where every body was at each of the last `REWIND` steps (`step % REWIND`; buffers reused).
    rewind: Vec<(u64, Vec<BodyAt>)>,
    /// Every player in the world: who, where, how fast (reused: `tracks`).
    far_players: Vec<(u32, glam::DVec3, Vec3)>,
    /// (reused)
    cmds_in: Vec<Cmd>,
    due: Vec<(usize, u64, Act)>,
    moved: Vec<(u64, usize, f64)>,
    encoded: Vec<u8>,
    made: Vec<(u64, Vec<u8>)>,
    seen: Vec<(told::From, Seen)>,
    pinned: Vec<Vec<u64>>,
    /// The ships' digests of the newest step some were due (`net::checked`), and that step; room
    /// for what the machines keep, to take them.
    checks: Vec<(u64, Digest)>,
    checks_at: u64,
    nums: Vec<f64>,
    /// What the server's own hands do, when it takes effect: the step, the ship, the control,
    /// the value (`control`).
    hands: Vec<(u64, u64, u16, f64)>,
    /// What a player let fly by their hand: our number for it, who, theirs, and when (what is
    /// told of it goes to them by their number: `blasts::OWN`).
    own: Vec<(u32, u32, u32, u64)>,
    /// Where every structure is this step, for every player's interest to ask.
    index: Index,
    /// (reused)
    mine: Vec<(told::From, Seen)>,
    /// What the keys are made with (a secret of this run), and the id the next body left waiting
    /// takes (from the top down: never one of a connection's).
    keys: std::hash::RandomState,
    next_lost: u32,
    /// Times the game was saved (`save`): the newest of two slots is the one with more.
    pub saves: u64,
}

/// Why an act was refused (what the player reads).
const OUT_OF_REACH: &str = "fuera de alcance";
const NOT_ALLOWED: &str = "no permitido en este servidor";
const NOT_CARRIED: &str = "no lo llevas en las manos";
const TOO_FAST: &str = "demasiado deprisa";
const TAKEN: &str = "lo lleva otro";

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
            helds: Vec::new(),
            states: Vec::new(),
            far_players: Vec::new(),
            rewind: (0..REWIND).map(|_| (u64::MAX, Vec::new())).collect(),
            cmds_in: Vec::new(),
            due: Vec::new(),
            moved: Vec::new(),
            encoded: Vec::new(),
            made: Vec::new(),
            seen: Vec::new(),
            pinned: Vec::new(),
            checks: Vec::new(),
            checks_at: 0,
            nums: Vec::new(),
            hands: Vec::new(),
            own: Vec::new(),
            index: Index::default(),
            mine: Vec::new(),
            keys: std::hash::RandomState::new(),
            next_lost: u32::MAX,
            saves: 0,
        }
    }

    /// The players in the game, by id (not the bodies waiting for who was cut off).
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.peers.iter().filter(|p| p.lost.is_none()).map(|p| p.id)
    }

    /// The key of the body that has waited longest for whoever was cut off (a game of one's own
    /// taken up again: the one who plays it comes back to it).
    pub fn waiting_key(&self) -> Option<u64> {
        self.peers.iter().find(|p| p.lost.is_some()).map(|p| p.key)
    }

    /// The bodies waiting for who was cut off, by the id they go by meanwhile.
    pub fn waiting(&self) -> impl Iterator<Item = u32> + '_ {
        self.peers.iter().filter(|p| p.lost.is_some()).map(|p| p.id)
    }

    /// A key no one could guess.
    fn new_key(&self, id: u32) -> u64 {
        use std::hash::BuildHasher;
        self.keys.hash_one((id, self.game.step, self.stats.steps, self.peers.len(), self.next_lost))
    }

    /// Player `id`'s body and hands as the server has them.
    pub fn player(&self, id: u32) -> Option<&Player> {
        self.peers.iter().position(|p| p.id == id).map(|k| &self.players[k])
    }

    /// Where every body in the world was at the end of step `step`, if that is one of the last
    /// `REWIND`: what the shooter saw, to judge a shot at a person by (lag compensation).
    pub fn bodies_at(&self, step: u64) -> Option<&[BodyAt]> {
        let (at, bodies) = &self.rewind[(step % REWIND as u64) as usize];
        (*at == step).then_some(&bodies[..])
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
        let key = self.new_key(id);
        let mut peer = Peer::new(id, key);
        peer.now = self.game.step;
        peer.tell(&Event::Hello { step: self.game.step, you: id, sun: self.game.sun, region: self.config.region, key });
        // (the ground as it is: every crater dug so far)
        for (b, body) in self.game.bodies.iter() {
            for e in net::ground(b, body.deform().craters()) {
                peer.tell(&e);
            }
        }
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

    /// Player `id` is gone: their body waits for them where it is, standing still and seen by
    /// the others, `HostConfig::keep` s (what was in its hands let go), for them to come back to
    /// it by their key (`Act::Back`); if it never was in the world, it goes with them.
    pub fn leave(&mut self, id: u32) {
        let Some(k) = self.peers.iter().position(|p| p.id == id && p.lost.is_none()) else { return };
        if self.players[k].away || self.config.keep <= 0.0 {
            self.peers.remove(k);
            self.players.remove(k);
            return;
        }
        self.players[k].hands.release(&mut self.game.builds);
        let until = self.game.step + (self.config.keep / STEP).ceil() as u64;
        let lost = self.next_lost;
        self.next_lost -= 1;
        let peer = &mut self.peers[k];
        (peer.id, peer.lost) = (lost, Some(until));
        peer.acts.clear();
        peer.interest.clear();
        (peer.newest, peer.heard, peer.behind) = (None, None, None);
        peer.untold();
        peer.quick.clear();
    }

    /// Who came in as player `k` was here before, as the one given `key`: their body back, as it
    /// waited (`Event::Back`; with nothing, if there is no such body or they had begun anew).
    fn back(&mut self, k: usize, key: u64) {
        let step = self.game.step.saturating_sub(1);
        let mut state = Vec::new();
        let found = self.peers.iter().position(|p| p.lost.is_some() && p.key == key);
        if let Some(j) = found
            && self.players[k].away
            && self.peers[k].newest.is_none()
        {
            self.players.swap(k, j);
            let was = &mut self.peers[j];
            let (drive, last, key) = (std::mem::take(&mut was.drive), was.last, was.key);
            self.peers.remove(j);
            self.players.remove(j);
            let k = if j < k { k - 1 } else { k };
            let peer = &mut self.peers[k];
            (peer.drive, peer.last, peer.key) = (drive, last, key);
            let p = &mut self.players[k];
            // (out of the world until its game is in it again, as anyone who comes in)
            p.away = true;
            p.pilot.write_state(&mut state);
            self.stats.back += 1;
            self.peers[k].tell(&Event::Back { step, state });
            return;
        }
        self.peers[k].tell(&Event::Back { step, state });
    }

    /// The game kept into `out` (`save`): its world and every body in it, of who is here and of
    /// who was cut off, by the key each is theirs by. `scenario`: the fingerprint of its data.
    pub fn save(&mut self, scenario: u32, out: &mut Vec<u8>) {
        self.saves += 1;
        let mut bodies: Vec<(u64, Vec<u8>)> = Vec::with_capacity(self.peers.len());
        for (peer, p) in self.peers.iter().zip(&self.players) {
            if p.away && peer.lost.is_none() {
                continue;
            }
            let mut state = Vec::with_capacity(256);
            p.pilot.write_state(&mut state);
            bodies.push((peer.key, state));
        }
        crate::save::write(&self.game, self.saves, scenario, bodies.iter().map(|(k, b)| (*k, &b[..])), out);
    }

    /// The game kept in `data` (`save`) taken up again in `game` (as its scenario starts): its
    /// world as it was, and every body in it waiting `config.keep` s for whoever comes back to it.
    pub fn load(mut game: Game, def: PlayerDef, config: HostConfig, scenario: u32, data: &[u8]) -> Result<Host, String> {
        let kept = crate::save::read(&mut game, scenario, data)?;
        let mut host = Host::new(game, def, config);
        host.saves = kept.saves;
        let until = host.game.step + (host.config.keep / STEP).ceil() as u64;
        for (key, state) in kept.bodies {
            let mut p = Player::new(host.game.bodies.clone(), &host.game.site, def);
            if p.pilot.read_state(&state).is_err() {
                continue;
            }
            let mut peer = Peer::new(host.next_lost, key);
            host.next_lost -= 1;
            peer.lost = Some(until);
            peer.last = Cmd { yaw: p.pilot.yaw, pitch: p.pilot.pitch, pack: p.pilot.pack_on, steady: p.pilot.steady, lamps: p.pilot.lamps, fly: p.pilot.flying, ..Cmd::default() };
            host.players.push(p);
            host.peers.push(peer);
        }
        Ok(host)
    }

    /// The bodies nobody came back for in time, gone.
    fn forget(&mut self) {
        let s = self.game.step;
        let mut k = 0;
        while k < self.peers.len() {
            if self.peers[k].lost.is_some_and(|t| t <= s) {
                self.players[k].hands.release(&mut self.game.builds);
                self.peers.remove(k);
                self.players.remove(k);
                self.stats.forgotten += 1;
            } else {
                k += 1;
            }
        }
    }

    /// Something player `from`'s game said (`net::CMDS` or `net::ACT`).
    pub fn take(&mut self, from: u32, data: &[u8]) {
        let Some(k) = self.peers.iter().position(|p| p.id == from) else { return };
        self.peers[k].now = self.game.step;
        let mut r = Reader::new(data);
        let ok = match r.u8() {
            // (no faster than a player's game sends them: the rest is not read)
            Ok(CMDS | ACT) if self.peers[k].flooded(data[0]) => {
                self.stats.flooded += 1;
                true
            }
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
                Ok((_, Act::Back { key })) => {
                    self.back(k, key);
                    true
                }
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
        // (how early the oldest of those not heard before came: what their game's clock is set
        // by. Not the newest: a game at few frames a second sends several steps at once, and the
        // first of them is the one that must be here in time)
        let mut first = None;
        for c in &self.cmds_in {
            if peer.heard.is_none_or(|h| c.step > h) {
                first = Some(first.map_or(c.step, |f: u64| f.min(c.step)));
            }
        }
        if let Some(f) = first {
            peer.lead = peer.lead.min(f as i64 - now as i64);
        }
        if let Some(c) = self.cmds_in.first() {
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
        // (what they said of the steps ahead was of the body before this: not compared)
        for c in &mut peer.claims {
            c.0 = 0;
        }
        peer.tell(&Event::Correct { step, state });
        self.stats.corrections += 1;
    }

    /// What is told from here on is as the game is now (`Peer::at`).
    fn stamp(&mut self) {
        let now = self.game.step;
        for peer in &mut self.peers {
            peer.now = now;
        }
    }

    /// One step of the game, and what each player is to be told of it (`send`).
    pub fn step(&mut self) {
        self.stamp();
        let s = self.game.step;
        self.forget();
        // ---- 1. what each asks of this step, and what each does that is due
        let cheats = self.config.cheats;
        self.due.clear();
        for (k, peer) in self.peers.iter_mut().enumerate() {
            let p = &mut self.players[k];
            let cmd = match peer.cmd_at(s).copied() {
                // (waiting for whoever was cut off: standing still, looking where they looked)
                _ if peer.lost.is_some() => {
                    let mut c = peer.last;
                    (c.step, c.input, c.keys, c.trigger) = (s, crate::pilot::Input::default(), 0, false);
                    c
                }
                Some(c) => {
                    // (its first: in the world from this step, as it is in its own game)
                    p.away = false;
                    peer.guessing = 0;
                    c
                }
                // (none yet: not in the world)
                None if p.away => continue,
                None => {
                    self.stats.guessed += 1;
                    peer.guessing += 1;
                    let mut c = peer.last;
                    (c.step, c.input.jump) = (s, false);
                    if peer.guessing > GUESS_MOST {
                        (c.input, c.keys, c.trigger) = (crate::pilot::Input::default(), 0, false);
                    }
                    c
                }
            };
            apply(&cmd, p, cheats);
            peer.last = cmd;
            // (what may be mended: as fast as the welder in the hands, nothing with none)
            let mend = match self.game.gear.mends(cmd.tool) {
                Some((rate, _)) => rate * MEND_SLACK,
                None if cheats => MEND_RATE,
                None => 0.0,
            };
            (peer.launches, peer.mend) = ((peer.launches + self.config.launches * STEP as f32).min(self.config.launches), (peer.mend + mend * STEP as f32).min(mend));
            peer.cmds_left = (peer.cmds_left + CMDS_RATE).min(CMDS_BURST);
            peer.acts_left = (peer.acts_left + ACTS_RATE).min(ACTS_BURST);
            peer.resyncs_left = (peer.resyncs_left + RESYNC_RATE).min(RESYNC_BURST);
            while peer.acts.front().is_some_and(|a| a.0 <= s) {
                if let Some((at, a)) = peer.acts.pop_front() {
                    self.due.push((k, at, a));
                }
            }
        }
        let mut due = std::mem::take(&mut self.due);
        for (k, at, a) in due.drain(..) {
            let fired = if let Act::Launch(_, tag) = a { Some(tag) } else { None };
            if let Err(why) = self.act(k, at, a) {
                self.stats.denied += 1;
                self.peers[k].tell(&Event::Denied(why.to_string()));
                // (what flies in their game already is gone from it)
                if let Some(tag) = fired {
                    self.peers[k].tell(&Event::Unfired { tag });
                }
            }
        }
        self.due = due;
        // ---- 2. what the server's own hands do now, and the seats' keys on their controls
        if !self.hands.is_empty() {
            let mut i = 0;
            while i < self.hands.len() {
                let (at, ship, control, value) = self.hands[i];
                if at <= s {
                    controls::set(&mut self.game.ships, &self.game.builds.set, ship, usize::from(control), value);
                    self.hands.swap_remove(i);
                } else {
                    i += 1;
                }
            }
        }
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
        self.stamp();
        // (where each body is, kept a moment: `bodies_at`)
        let slot = &mut self.rewind[(self.game.step % REWIND as u64) as usize];
        slot.0 = self.game.step;
        slot.1.clear();
        for (p, peer) in self.players.iter().zip(&self.peers) {
            if !p.away {
                let (feet, up) = p.pilot.feet();
                slot.1.push(BodyAt { id: peer.id, feet, up, eye: p.pilot.eye(), ride: p.pilot.ride.map(|r| (r.id, r.local)) });
            }
        }
        // ---- 5. each body against what its game got
        for k in 0..self.peers.len() {
            if self.players[k].away || self.peers[k].lost.is_some() {
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
        self.craters();
        self.holds();
        self.strikes();
        self.know();
        self.seen();
        self.said();
        // ---- 7. what goes out
        self.snapshots();
        self.tracks();
    }

    /// What every player is to be told of the last step: `send(to, reliable, bytes)`, which says
    /// whether it was taken. Events not taken (the player's connection is not through yet) are
    /// kept for the next time.
    pub fn send(&mut self, mut send: impl FnMut(u32, bool, &[u8]) -> bool) {
        for peer in &mut self.peers {
            if peer.lost.is_some() {
                // (nobody to tell)
                peer.untold();
                peer.quick.clear();
                continue;
            }
            // (as many messages as it takes, each as many events as fit; what is not taken is kept)
            let (mut from, mut k) = (0, 0);
            while k < peer.ends.len() {
                let mut j = k;
                // (of one step: each message says the step of what it carries)
                while j < peer.ends.len() && peer.ends[j] - from <= EVENTS_ROOM && j - k < EVENTS_MOST && peer.at[j] == peer.at[k] {
                    j += 1;
                }
                if j == k {
                    // (one event too big for any message: it cannot go)
                    self.stats.too_big += 1;
                    (from, k) = (peer.ends[k], k + 1);
                    continue;
                }
                let end = peer.ends[j - 1];
                net::events_message(peer.at[k], (j - k) as u32, &peer.events[from..end], &mut peer.sure);
                let sent = send(peer.id, true, &peer.sure);
                peer.sure.clear();
                if !sent {
                    break;
                }
                self.stats.event_bytes += (end - from) as u64;
                (from, k) = (end, j);
            }
            if k > 0 {
                peer.events.drain(..from);
                peer.ends.drain(..k);
                peer.at.drain(..k);
                for e in &mut peer.ends {
                    *e -= from;
                }
            }
            if !peer.quick.is_empty() {
                send(peer.id, false, &peer.quick);
                peer.quick.clear();
            }
            if !peer.tracks.is_empty() {
                send(peer.id, false, &peer.tracks);
                peer.tracks.clear();
            }
        }
    }

    /// Event `e` to everyone who knows structure `id` but player `but`.
    /// A hand of the server's own on control `id` of ship `ship` (a ship the world flies, a
    /// script, a test): set to `value` as a player's would set it, `net::HAND_LEAD` steps from now,
    /// and told now to every game that knows the ship, for that step: in all of them it is done at
    /// the same step. False if the ship or the control is not there.
    pub fn control(&mut self, ship: u64, id: &str, value: f64) -> bool {
        let Some(n) = self.game.ships.by_structure(ship) else { return false };
        let Some(c) = self.game.ships.list[n].panels.controls.iter().position(|c| c.id == id) else { return false };
        let at = self.game.step + net::HAND_LEAD;
        self.hands.push((at, ship, c as u16, value));
        for peer in &mut self.peers {
            peer.now = at;
        }
        self.tell_knowing(ship, &Event::Control { ship, control: c as u16, value }, None);
        self.stamp();
        true
    }

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
    fn act(&mut self, k: usize, at: u64, a: Act) -> Result<(), &'static str> {
        // (one that came late — said again after it was lost on the way — is judged where the body
        // was at its step, as its game had it: kept a moment, `bodies_at`)
        let late = self.game.step.saturating_sub(at);
        let then = if late > 0 {
            let id = self.peers[k].id;
            self.bodies_at(at).and_then(|b| b.iter().find(|b| b.id == id)).map(|b| self.players[k].pilot.eye() - b.eye)
        } else {
            None
        };
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
                // (what another has in their hands is theirs)
                let held: Vec<u64> = self.players.iter().enumerate().filter(|(j, _)| *j != k).filter_map(|(_, o)| o.hands.holding()).collect();
                let p = &mut self.players[k];
                let view = p.acting_view(set);
                let on = p.pilot.ride.map(|r| r.id);
                if p.hands.reach(set, &g.ships, &view, on).is_some_and(|(r, _, _)| held.contains(&r.id)) {
                    // (their game took it already: let it go there too)
                    self.peers[k].tell(&Event::Unheld);
                    return Err(TAKEN);
                }
                p.hands.grab(set, &g.ships, &view, on)?;
            }
            Act::Release => p.hands.release(&mut g.builds),
            Act::Wheel(n) => {
                p.hands.wheel(f64::from(n));
            }
            Act::Launch(mut l, theirs) => {
                let peer = &mut self.peers[k];
                // (what the hands carry, a reload's worth apart; anything else, at the pace of its
                // kind, only where tests are let be: a test key)
                let every = match (l.what, g.gear.fires(peer.last.tool)) {
                    (crate::blasts::What::Shot(i), Some((shot, reload))) if i == shot => (reload / STEP).floor() as u64,
                    _ if !self.config.cheats => return Err(NOT_CARRIED),
                    (crate::blasts::What::Shot(i), _) => g.blasts.shot_rate(i).map_or(RELOAD, |r| (1.0 / f64::from(r.max(0.1)) / STEP).floor() as u64),
                    _ => RELOAD,
                };
                let last = peer.fired.iter().position(|f| f.0 == l.what);
                let now = g.step;
                if peer.launches < 1.0 || last.is_some_and(|i| now < peer.fired[i].1 + every.saturating_sub(every / 5)) {
                    return Err(TOO_FAST);
                }
                match last {
                    Some(i) => peer.fired[i].1 = now,
                    None => peer.fired.push((l.what, now)),
                }
                // (from the hand, going as the body goes, at the speed its kind has; if it came late,
                // from where the hand was then, gone on with the body since)
                if l.from.distance(eye - then.unwrap_or(glam::DVec3::ZERO)) > HAND_REACH {
                    return Err(OUT_OF_REACH);
                }
                if then.is_some() {
                    l.from += l.vel * (late as f64 * STEP);
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
                let told = g.blasts.seen.len();
                if !g.blasts.launch(l, &bodies, &mut g.builds) {
                    return Err(TOO_FAST);
                }
                // (whose it is, and their number for it: theirs flies already in their game)
                if let Some(&Seen::Launch { tag, .. }) = g.blasts.seen.get(told) {
                    let id = self.peers[k].id;
                    self.own.retain(|o| o.0 != tag);
                    self.own.push((tag, id, theirs & !crate::blasts::OWN, g.step));
                }
            }
            Act::Mend { structure, part, hp } => {
                let peer = &mut self.peers[k];
                // (with a welder in the hands: what it gives is `peer.mend`)
                if !self.config.cheats && g.gear.mends(peer.last.tool).is_none() {
                    return Err(NOT_CARRIED);
                }
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
                // (with a welder in the hands, one part at a time, as long as it takes)
                let peer = &mut self.peers[k];
                if !self.config.cheats {
                    let Some((_, takes)) = g.gear.mends(peer.last.tool) else { return Err(NOT_CARRIED) };
                    let apart = (f64::from(takes) * REBUILD_SLACK / STEP) as u64;
                    if peer.rebuilt.is_some_and(|at| g.step < at + apart) {
                        return Err(TOO_FAST);
                    }
                }
                let lib = g.builds.set.lib.clone();
                let Some(i) = g.builds.set.index_of(structure) else { return Err(OUT_OF_REACH) };
                let s = &mut g.builds.set.list[i];
                let Some(pt) = s.parts.get(part as usize) else { return Err(OUT_OF_REACH) };
                if s.to_world(pt.local.translation.into()).distance(eye) > HAND_REACH + f64::from(pt.shape.sphere().1) {
                    return Err(OUT_OF_REACH);
                }
                if s.rebuild(&lib.catalog, part as usize, REBUILT) {
                    peer.rebuilt = Some(g.step);
                }
            }
            Act::Spawn { kind, pos, rot } => {
                if !self.config.cheats {
                    return Err(NOT_ALLOWED);
                }
                g.ships.spawn_free(&mut g.builds, &kind, pos, rot).map_err(|_| NOT_ALLOWED)?;
            }
            // (taken as it comes, not at a step: `take`)
            Act::Back { .. } => return Err(NOT_ALLOWED),
            Act::Body(state) => {
                if !self.config.cheats {
                    return Err(NOT_ALLOWED);
                }
                p.pilot.read_state(&state).map_err(|_| NOT_ALLOWED)?;
            }
            Act::Resync { id } => {
                // (told anew: as if it had just come to be known; not in a loop)
                let peer = &mut self.peers[k];
                if peer.resyncs_left < 1.0 {
                    self.stats.flooded += 1;
                    return Err(TOO_FAST);
                }
                peer.resyncs_left -= 1.0;
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
        let (set, now) = (&self.game.builds.set, self.game.step);
        // (kept in the order of the ids, as the structures are)
        self.shadows.retain(|sh| set.index_of(sh.0).is_some());
        for s in &set.list {
            let i = match self.shadows.binary_search_by_key(&s.id, |sh| sh.0) {
                Ok(i) => i,
                Err(i) => {
                    self.shadows.insert(i, (s.id, s.version, Shadow::of(s), u64::MAX));
                    continue;
                }
            };
            // (what changed, as far as it is worth telling; once it is left as it is, the rest
            // of it: every copy ends alike)
            let sh = &mut self.shadows[i];
            let mut delta = Vec::new();
            let told = if sh.1 != s.version {
                (sh.1, sh.3) = (s.version, now + SETTLE);
                sh.2.delta(s, &mut delta)
            } else if now >= sh.3 {
                sh.3 = u64::MAX;
                sh.2.settle(s, &mut delta)
            } else {
                continue;
            };
            if told {
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

    /// What came to be held or was let go in the last step, to whoever knows it, and the ships that
    /// took or let go of it all over again (what their clamps hold is in their systems).
    fn holds(&mut self) {
        let set = &self.game.builds.set;
        self.helds.retain(|h| set.index_of(h.0).is_some());
        let mut ships: Vec<u64> = Vec::new();
        for s in &set.list {
            let i = match self.helds.binary_search_by_key(&s.id, |h| h.0) {
                Ok(i) => i,
                Err(i) => {
                    self.helds.insert(i, (s.id, s.held));
                    continue;
                }
            };
            if self.helds[i].1 == s.held {
                continue;
            }
            let was = std::mem::replace(&mut self.helds[i].1, s.held);
            ships.extend(was.map(|h| h.by).into_iter().chain(s.held.map(|h| h.by)));
            self.encoded.clear();
            net::append_event(&Event::Hold { id: s.id, held: s.held }, &mut self.encoded);
            for peer in &mut self.peers {
                if peer.interest.knows(s.id) {
                    peer.tell_bytes(&self.encoded);
                }
            }
        }
        ships.sort_unstable();
        ships.dedup();
        for ship in ships {
            let (Some(n), Some(s)) = (self.game.ships.by_structure(ship), self.game.builds.set.get(ship)) else { continue };
            let mut data = Vec::new();
            lunar_ship::sync::write_ship(&self.game.ships.list[n], s, &|x| Some(x), &mut data);
            self.tell_knowing(ship, &Event::Systems { ship, data }, None);
        }
    }

    /// The craters dug in the last step, to everyone (the ground is everyone's), in the order dug.
    fn craters(&mut self) {
        for k in 0..self.game.out.craters.len() {
            let (body, crater) = self.game.out.craters[k];
            self.encoded.clear();
            net::append_event(&Event::Crater { body, crater }, &mut self.encoded);
            for peer in &mut self.peers {
                peer.tell_bytes(&self.encoded);
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
        self.index.build(&rule, set);
        let index = &self.index;
        let players = &self.players;
        let pinned = &self.pinned;
        let scenario_end = self.game.builds.scenario_end;
        let ships = &self.game.ships;
        self.peers.par_iter_mut().enumerate().with_min_len(2).for_each(|(k, peer)| {
            let p = &players[k].pilot;
            peer.went.clear();
            if peer.lost.is_some() {
                (peer.came.clear(), peer.rested.clear());
                return;
            }
            let from = peer.came.len();
            peer.interest.update_in(&rule, set, index, p.position, p.velocity_in(set), &pinned[k], dt, &mut peer.came, &mut peer.went);
            peer.interest.owe(set, p.position, &pinned[k], dt);
            peer.rested.clear();
            peer.interest.rests(set, &mut peer.rested);
            // (what their game has as the server does, from the start, is not told: what the
            // scenario set, anchored, untouched)
            let mut i = from;
            while i < peer.came.len() {
                let id = peer.came[i];
                let untouched = id < scenario_end && set.get(id).is_some_and(|s| s.anchored && s.version == 0) && ships.by_structure(id).is_none();
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
                self.peers[k].tell_bytes(&self.made[at].1);
            }
            let mut came = came;
            came.clear();
            self.peers[k].came = came;
            let went = std::mem::take(&mut self.peers[k].went);
            for &id in &went {
                self.peers[k].tell(&Event::Gone { id });
            }
            self.peers[k].went = went;
            // (what came to rest, where it rests, to the last bit)
            let rested = std::mem::take(&mut self.peers[k].rested);
            for &id in &rested {
                if let Some(s) = self.game.builds.set.get(id) {
                    let e = Event::Rest { id, pos: s.pos, rot: s.rot, grounded: s.grounded, legs: s.springs.iter().map(|sp| (sp.x, sp.load)).collect() };
                    self.peers[k].tell(&e);
                }
            }
            self.peers[k].rested = rested;
            // (all they were to know when they came is told before this: their world is there)
            if !self.peers[k].ready && self.peers[k].lost.is_none() {
                self.peers[k].ready = true;
                self.peers[k].tell(&Event::Ready);
            }
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
        // (what is a player's own, by our number: who, and theirs)
        let tag_of = |s: &Seen| match *s {
            Seen::Launch { tag, .. } | Seen::End { tag, .. } | Seen::Track { tag, .. } => tag,
        };
        let encode = |chunk: &[(told::From, Seen)], out: &mut Vec<u8>| {
            let mut bytes = vec![0u8; 64 + chunk.len() * 96];
            let mut w = Writer::new(&mut bytes);
            told::write_seen(&mut w, stamp, chunk, |id| Some(Named::Built(id)));
            let n = w.finish().unwrap_or(0);
            bytes.truncate(n);
            out.clear();
            net::append_event(&Event::Seen(bytes), out);
        };
        let mut seen = std::mem::take(&mut self.seen);
        for chunk in seen.chunks(told::SEEN_EACH) {
            encode(chunk, &mut self.encoded);
            // (to whoever is near enough to see any of it)
            let at = |s: &Seen| match s {
                Seen::Launch { launch, .. } => launch.from,
                Seen::End { at, .. } => *at,
                Seen::Track { pos, .. } => *pos,
            };
            let owners = chunk.iter().filter_map(|(_, s)| self.own.iter().find(|o| o.0 == tag_of(s)).map(|o| o.1)).collect::<Vec<u32>>();
            for k in 0..self.peers.len() {
                let eye = self.players[k].pilot.position;
                if !chunk.iter().any(|(_, s)| at(s).distance(eye) < reach) {
                    continue;
                }
                let id = self.peers[k].id;
                if !owners.contains(&id) {
                    self.peers[k].tell_bytes(&self.encoded);
                    continue;
                }
                // (theirs, by their number: its start they have; its end and where it is, they are told)
                self.mine.clear();
                for &(from, s) in chunk {
                    let Some(&(_, _, theirs, _)) = self.own.iter().find(|o| o.0 == tag_of(&s) && o.1 == id) else {
                        self.mine.push((from, s));
                        continue;
                    };
                    let t = theirs | crate::blasts::OWN;
                    match s {
                        Seen::Launch { .. } => {}
                        Seen::End { what, at, dir, vel, on, extra, .. } => self.mine.push((from, Seen::End { tag: t, what, at, dir, vel, on, extra })),
                        Seen::Track { pos, vel, push, .. } => self.mine.push((from, Seen::Track { tag: t, pos, vel, push })),
                    }
                }
                if !self.mine.is_empty() {
                    let mut mine = Vec::new();
                    encode(&self.mine, &mut mine);
                    self.peers[k].tell_bytes(&mine);
                }
            }
        }
        // (what ended is no one's any more; nor what was let fly long ago)
        let now = self.game.step;
        for (_, s) in &seen {
            if let Seen::End { tag, .. } = s {
                self.own.retain(|o| o.0 != *tag);
            }
        }
        self.own.retain(|o| now < o.3 + (600.0 / STEP) as u64);
        seen.clear();
        self.seen = seen;
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
    /// What each player knows only from afar, every `TRACK_EVERY` steps (theirs in turn): the
    /// ships they do not know in full and the players past what a snapshot carries, out to
    /// `TRACK_REACH`, the nearest first, as many as a datagram holds.
    fn tracks(&mut self) {
        let step = self.game.step;
        let (set, bodies, kinds) = (&self.game.builds.set, &*self.game.bodies, &self.game.ships.kinds);
        let (room, most) = (self.config.snap_room, self.config.rule.most);
        self.far_players.clear();
        self.far_players.extend(self.players.iter().zip(&self.peers).filter(|(p, _)| !p.away).map(|(p, peer)| (peer.id, p.pilot.position, p.pilot.velocity().as_vec3())));
        for (k, peer) in self.peers.iter_mut().enumerate() {
            if (step + k as u64) % TRACK_EVERY != 0 || peer.lost.is_some() {
                continue;
            }
            let me = self.players[k].pilot.position;
            peer.far.clear();
            for sh in &self.game.ships.list {
                let Some(s) = set.get(sh.structure).filter(|s| !peer.interest.knows(s.id)) else { continue };
                let (pos, vel, _) = coasted(s, bodies.field(s.pos).pull, set.now - s.clock);
                if pos.distance(me) <= TRACK_REACH {
                    let kind = kinds.iter().position(|kd| kd.id == sh.kind.id).unwrap_or(0) as u16;
                    peer.far.push(Far { what: FarWhat::Ship { id: s.id, kind }, pos, vel: vel.as_vec3() });
                }
            }
            for &(id, pos, vel) in &self.far_players {
                let d = pos.distance(me);
                if id != peer.id && d >= most && d <= TRACK_REACH {
                    peer.far.push(Far { what: FarWhat::Player(id), pos, vel });
                }
            }
            if peer.far.is_empty() && !peer.far_told {
                continue;
            }
            peer.far.sort_by(|a, b| a.pos.distance_squared(me).total_cmp(&b.pos.distance_squared(me)));
            peer.far_told = !peer.far.is_empty();
            net::write_tracks(step, me, &peer.far, room, &mut peer.tracks);
        }
    }

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
        // (the digests of the ships due this step: each player's game takes its own of the same
        // step, and they go to each in their next snapshot)
        let step = self.game.step;
        if step % net::CHECK_EVERY == 0 {
            self.checks.clear();
            for sh in &self.game.ships.list {
                if net::checked(sh.structure, step)
                    && sh.pace == Pace::Full
                    && let Some(s) = set.get(sh.structure)
                {
                    self.checks.push((s.id, Digest::of_with(Some(sh), s, &mut self.nums)));
                }
            }
            self.checks_at = step;
        }
        let (checks, checks_at) = (&self.checks, self.checks_at);
        let (every, room, rule) = (self.config.snap_every.max(1), self.config.snap_room, self.config.rule);
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
                if (step + k as u64) % every != 0 || peer.lost.is_some() {
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
                snap.checks.clear();
                if peer.checked < checks_at {
                    let aboard = me.ride.map(|r| r.id).or(me.seat.map(|x| x.structure));
                    let near = |id: u64| set.get(id).is_some_and(|s| net::checks_near(s, me.position, aboard));
                    snap.checks.extend(checks.iter().filter(|c| peer.interest.knows(c.0) && near(c.0)).take(net::MOST_CHECKS as usize));
                    (snap.checks_at, peer.checked) = (checks_at, checks_at);
                }
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
    (p.tool, p.trigger) = (c.tool, c.trigger);
    // (a look as a head can: up and down no further than its neck, round no further than a
    // number that still turns finely)
    let most = p.pilot.look_limit();
    (p.pilot.yaw, p.pilot.pitch) = (c.yaw.clamp(-1e6, 1e6), c.pitch.clamp(-most, most));
    (p.pilot.pack_on, p.pilot.steady, p.pilot.lamps) = (c.pack, c.steady, c.lamps);
    if cheats && c.fly != p.pilot.flying {
        p.pilot.toggle_flight();
    }
    // (floating, the mouse turned the whole body: as the player left it)
    if let Some([up, fore]) = c.frame {
        p.pilot.set_body_frame(up.as_dvec3(), fore.as_dvec3());
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
            sync::write_ship(sh, s, &|x| Some(x), &mut systems);
            (sh.kind.id.clone(), sh.seed, systems)
        }
        None => (String::new(), 0, Vec::new()),
    };
    let (pos, vel, rot) = coasted(s, g.bodies.field(s.pos).pull, set.now - s.clock);
    // (at rest, still: it is to rest where it is in every game)
    let (vel, spin) = if s.resting { (Vec3::ZERO, Vec3::ZERO) } else { (vel.as_vec3(), s.spin) };
    Some(Event::Made { id, lineage: s.lineage, born: s.born, ship, seed, pos, rot, vel, spin, resting: s.resting, held: s.held, make, state, systems })
}
