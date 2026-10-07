//! A player's side of a game: connects to a server, sends this player's state and that of the
//! things it holds, and keeps what the server passes on of the others, ready to be drawn.
//! - `receive`: what comes in (the welcome, the others' states, events);
//! - `send`: what goes out (the hello, our states at the tick, pings);
//! - `delay`: how far in the past the others are drawn.
//!
//! Use: `connect`, then every frame `set_player` (and `set_rigid` for each thing `owns_thing`
//! says is ours and is moving), `update(now)`, drain `events()`, and draw `players(now, ..)` and
//! `rigid_now(id, now, ..)`. The same clock, in seconds, for every `now` (`lunar_net::now()` is
//! one). What the game says now and then goes with `tell` (to everyone, reliably and in order),
//! `hint` (to everyone, may be lost) and `tell_to` (to one player); what must be one player's at
//! a time is asked for with `claim`.
mod delay;
mod receive;
mod send;

use crate::channel::{Channel, ChannelStats, Inbox, MAX_MESSAGE, MAX_UNRELIABLE};
use crate::clock::ServerClock;
use crate::game::{PlayerState, RigidState, key};
use crate::proto::{Datagram, FRAMING, MAX_BUILD, Msg, lead};
use crate::snap::{EXTRAPOLATE, SnapBuffer};
use crate::text;
use crate::throttle::Throttle;
use crate::transport::{Addr, Transport, Udp};
use crate::wire::Writer;
use delay::Delay;
use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Asking the server to let us in.
    Connecting,
    /// In. `players`: everyone connected, us too.
    Connected { id: u32, players: usize, ping_ms: f32 },
    /// Not connected, and why: refused, no answer, thrown out, connection lost, or closed by us.
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Someone is in the game: they came, or they were here when we came.
    Joined { id: u32, name: String },
    /// Someone is gone.
    Left { id: u32, name: String },
    /// Who the host is now: what nobody asks for is theirs to simulate.
    Host { player: Option<u32> },
    /// A key changed hands among those who ask for it (`None`: nobody asks for it any more).
    Owner { key: u64, player: Option<u32> },
    /// Something a player told everyone (ours too, if we asked for it back: `tell_all`), in the
    /// order the server passed it on, which is the same for everyone.
    Told { by: u32, data: Vec<u8> },
    /// Something a player told everyone that may not arrive, or arrive late (`hint`).
    Hinted { by: u32, data: Vec<u8> },
    /// Something a player sent to us alone (`tell_to`).
    Direct { by: u32, data: Vec<u8> },
    /// A chat line (ours too, so everyone reads them in the same order); `None`: the server speaks.
    Chat { from: Option<u32>, text: String },
    /// The server has told us what it knows (who is here, the host, who holds what). What the
    /// world is like now is for the game to ask of those who hold it.
    Synced,
    /// We were connected and no longer are.
    Disconnected { reason: String },
}

enum Phase {
    Hello,
    Live,
    Over(String),
}

struct Peer {
    id: u32,
    name: String,
    snaps: SnapBuffer<PlayerState>,
}

#[derive(Default)]
struct ThingSlot {
    /// What its holder sends, when that is not us: its states, and its joints as last heard (they
    /// come apart, only when they change; each state that arrives takes them as they are then).
    snaps: SnapBuffer<RigidState>,
    joints: Vec<f32>,
    /// What we send, when it is ours: the latest given to `set_rigid`.
    mine: RigidState,
    /// Given since the last send.
    set: bool,
    /// It is among those looked at each send (`Client::live`).
    listed: bool,
    throttle: Throttle,
    joints_throttle: Throttle,
}

/// Things a game may have, whatever the server says: a flood of wild ids must not make us reserve memory for them.
const MAX_THINGS: usize = 1 << 16;
/// Seconds between two hellos, and how long we keep saying hello.
const HELLO_EVERY: f64 = 0.25;
const HELLO_FOR: f64 = 5.0;
/// Seconds of silence from the server after which the connection is given up.
const TIMEOUT: f64 = 10.0;
/// The stream of a thing's hull, and of its joints.
pub(crate) const HULL: u8 = 0;
pub(crate) const JOINTS: u8 = 1;

/// The most bytes `tell`, `tell_all` and `tell_to` take in one go.
/// What a thing carried on past its newest snapshot may be taken to be speeding up by at most
/// (m/s²): more is a jump told as a change of speed, not a push.
pub const MAX_ACCEL: f32 = 150.0;
pub const MAX_TELL: usize = MAX_MESSAGE - FRAMING;
/// The most bytes `hint` takes.
pub const MAX_HINT: usize = MAX_UNRELIABLE - FRAMING;

pub struct Client {
    transport: Box<dyn Transport>,
    server: Addr,
    phase: Phase,
    salt: u32,
    /// What the server's challenge said: the hello that is listened to carries it.
    cookie: u64,
    name: String,
    build: String,
    scenario: u32,
    started: Option<f64>,
    last_hello: f64,
    channel: Channel,
    id: Option<u32>,
    server_name: String,
    synced: bool,
    /// Seconds between two sends of our states.
    tick: f64,
    next_send: f64,
    clock: ServerClock,
    next_ping: f64,
    delay: Delay,
    last_update: f64,
    peers: Vec<Peer>,
    host: Option<u32>,
    /// Who holds each key somebody asked for.
    holders: HashMap<u64, u32>,
    /// The keys we asked for and have not let go.
    asked: Vec<u64>,
    things: HashMap<u64, ThingSlot>,
    /// The things of ours with something to send (set lately, or their last change still to go again).
    live: Vec<u64>,
    me: Option<PlayerState>,
    me_throttle: Throttle,
    /// A state was set since the last update: it is of the moment of the next one.
    set: bool,
    sampled: f64,
    events: Vec<Event>,
    // Buffers that go round.
    inbox: Inbox,
    datagram: Vec<u8>,
    msg: Vec<u8>,
    rigid: RigidState,
}

impl Client {
    /// Starts connecting to `addr` (`"host:port"`) over UDP and returns at once: `status()` says
    /// `Connecting` until the server answers. `name`: the player's; `build`: the game's version
    /// (it must be the server's game's); `scenario`: a number that says what world the game starts
    /// with (it must be the same as that of whoever started the game on the server).
    /// `Err` only if the address makes no sense or no socket can be opened.
    pub fn connect(addr: &str, name: &str, build: &str, scenario: u32) -> Result<Client, String> {
        let server = Udp::resolve(addr).map_err(|_| text::bad_address(addr))?;
        let udp = Udp::toward(server).map_err(|e| text::no_socket(&e.to_string()))?;
        Ok(Client::with_transport(Box::new(udp), Addr::Udp(server), name, build, scenario))
    }

    /// The same over any transport (tests use the in-memory one).
    pub fn with_transport(t: Box<dyn Transport>, server: Addr, name: &str, build: &str, scenario: u32) -> Client {
        // A number of our own for this connection; the hasher's seed is the system's randomness.
        let salt = RandomState::new().hash_one(std::process::id()) as u32;
        Client {
            transport: t,
            server,
            phase: Phase::Hello,
            salt,
            cookie: 0,
            name: text::clean_name(name),
            build: text::cut(build, MAX_BUILD).to_string(),
            scenario,
            started: None,
            last_hello: f64::NEG_INFINITY,
            channel: Channel::new(0.0, lead::DATA),
            id: None,
            server_name: String::new(),
            synced: false,
            tick: 0.05,
            next_send: 0.0,
            clock: ServerClock::new(),
            next_ping: 0.0,
            delay: Delay::new(),
            last_update: 0.0,
            peers: Vec::new(),
            host: None,
            holders: HashMap::new(),
            asked: Vec::new(),
            things: HashMap::new(),
            live: Vec::new(),
            me: None,
            me_throttle: Throttle::default(),
            set: false,
            sampled: 0.0,
            events: Vec::new(),
            inbox: Inbox::new(),
            datagram: vec![0; 2048],
            msg: Vec::new(),
            rigid: RigidState::default(),
        }
    }

    /// Call every frame, after `set_player` and `set_rigid`: receives, fills the events, and sends
    /// what is due (states at the tick rate; events and acks at once).
    pub fn update(&mut self, now: f64) {
        let dt = now - self.last_update;
        self.last_update = now;
        if self.set {
            (self.set, self.sampled) = (false, now);
        }
        if matches!(self.phase, Phase::Over(_)) {
            return;
        }
        self.receive(now);
        match self.phase {
            Phase::Hello => self.hello(now),
            Phase::Live => {
                self.clock.advance(dt);
                self.delay.advance(dt, self.tick);
                if self.channel.silence(now) > TIMEOUT {
                    self.over(text::LOST);
                    return;
                }
                self.send(now);
                self.channel.flush(now, self.server, self.transport.as_mut());
            }
            Phase::Over(_) => {}
        }
    }

    pub fn status(&self) -> Status {
        match &self.phase {
            Phase::Hello => Status::Connecting,
            Phase::Live => Status::Connected { id: self.id.unwrap_or(0), players: self.peers.len() + 1, ping_ms: self.channel.rtt() * 1000.0 },
            Phase::Over(reason) => Status::Failed(reason.clone()),
        }
    }

    /// Whether we are in (welcomed, and not gone since).
    pub fn connected(&self) -> bool {
        matches!(self.phase, Phase::Live)
    }

    /// Our id, once connected.
    pub fn id(&self) -> Option<u32> {
        self.id
    }

    /// Our player's latest state; it is sent at the tick rate (less often while it does not change).
    pub fn set_player(&mut self, s: &PlayerState) {
        self.me = Some(*s);
        self.set = true;
    }

    /// The latest state of a thing of ours that moves; ignored if the thing is not ours. A thing
    /// that is no longer given goes twice more as it was last and then not at all: at rest it
    /// costs nothing.
    pub fn set_rigid(&mut self, s: &RigidState) {
        if !self.owns_thing(s.id) || (self.things.len() >= MAX_THINGS && !self.things.contains_key(&s.id)) {
            return;
        }
        let slot = self.things.entry(s.id).or_default();
        slot.mine.set(s);
        slot.set = true;
        if !slot.listed {
            slot.listed = true;
            self.live.push(s.id);
        }
        self.set = true;
    }

    /// The host: what nobody asks for is theirs to simulate.
    pub fn host(&self) -> Option<u32> {
        self.host
    }

    /// Whether we are the host.
    pub fn hosting(&self) -> bool {
        self.id.is_some() && self.host == self.id
    }

    /// Whose a key is, as far as we have been told: its holder's among those who asked for it; of a
    /// thing nobody asked for, the host's.
    pub fn owner(&self, key: u64) -> Option<u32> {
        self.holders.get(&key).copied().or(if key::hosted(key) { self.host } else { None })
    }

    /// Who holds a key among those who asked for it (`None`: nobody asked; a thing is then the host's).
    pub fn holder(&self, key: u64) -> Option<u32> {
        self.holders.get(&key).copied()
    }

    /// Whether a key is ours.
    pub fn owns(&self, key: u64) -> bool {
        self.id.is_some() && self.owner(key) == self.id
    }

    /// Whether thing `id` is ours to simulate and send.
    pub fn owns_thing(&self, id: u64) -> bool {
        self.owns(key::thing(id))
    }

    /// Asks for a key: it is ours once `owner` says so (an `Event::Owner` comes), at once if nobody
    /// held it, when those before us let it go if somebody did. Asking twice is asking once.
    pub fn claim(&mut self, key: u64) {
        if matches!(self.phase, Phase::Live) && !self.asked.contains(&key) {
            self.asked.push(key);
            self.say(&Msg::Claim { key }, 16);
        }
    }

    /// Lets a key go (or stops waiting for it).
    pub fn release(&mut self, key: u64) {
        if let Some(at) = self.asked.iter().position(|k| *k == key) {
            self.asked.swap_remove(at);
            self.say(&Msg::Release { key }, 16);
        }
    }

    /// Whether we asked for a key and have not let it go.
    pub fn asked(&self, key: u64) -> bool {
        self.asked.contains(&key)
    }

    /// Something for everyone else, reliably and in order (at most `MAX_TELL` bytes). False if it
    /// could not be queued (not connected, too long).
    pub fn tell(&mut self, data: &[u8]) -> bool {
        data.len() <= MAX_TELL && self.say(&Msg::Tell { echo: false, data }, data.len())
    }

    /// The same, and back to us too (as an `Event::Told` of our own) in its place among what
    /// everyone told: for what must happen in one order everywhere.
    pub fn tell_all(&mut self, data: &[u8]) -> bool {
        data.len() <= MAX_TELL && self.say(&Msg::Tell { echo: true, data }, data.len())
    }

    /// Something for one player alone, reliably and in order.
    pub fn tell_to(&mut self, player: u32, data: &[u8]) -> bool {
        data.len() <= MAX_TELL && self.say(&Msg::To { player, data }, data.len())
    }

    /// Something for everyone else that may be lost (at most `MAX_HINT` bytes): what is over in a moment anyway.
    pub fn hint(&mut self, data: &[u8]) {
        if !matches!(self.phase, Phase::Live) || data.len() > MAX_HINT {
            return;
        }
        self.msg.resize(data.len() + FRAMING, 0);
        let mut w = Writer::new(&mut self.msg);
        Msg::Hint(data).encode(&mut w);
        if let Ok(n) = w.finish() {
            self.channel.send_unreliable(&self.msg[..n]);
        }
    }

    /// A chat line to everyone (it comes back to us as an `Event::Chat`). Ignored while not connected.
    pub fn chat(&mut self, text: &str) {
        let text = text::clean(text, text::CHAT_CHARS);
        if !text.is_empty() {
            self.say(&Msg::Chat { text: &text }, 1100);
        }
    }

    fn say(&mut self, msg: &Msg, room: usize) -> bool {
        if !matches!(self.phase, Phase::Live) {
            return false;
        }
        self.msg.resize(room + FRAMING, 0);
        let mut w = Writer::new(&mut self.msg);
        msg.encode(&mut w);
        let Ok(n) = w.finish() else { return false };
        if !self.channel.send_reliable(&self.msg[..n]) {
            self.over(text::LOST);
            return false;
        }
        true
    }

    /// What happened since the last call, in order.
    pub fn events(&mut self) -> std::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// The moment the others are drawn at, on the server's clock.
    fn drawn(&self, now: f64) -> f64 {
        self.clock.server_time(now) - self.delay.seconds()
    }

    /// The other players into `out` (emptied first), each as it was about 100 ms ago: between the
    /// two snapshots around that moment; with none newer, carried on by its velocity for at most
    /// 250 ms and then held. While a player rides a ship (`ride`), place it from `local` and your
    /// copy of the ship: its `pos` and `vel` may be up to a second old.
    pub fn players(&self, now: f64, out: &mut Vec<(u32, PlayerState)>) {
        out.clear();
        let t = self.drawn(now);
        for p in &self.peers {
            let mut s = PlayerState::default();
            if p.snaps.sample(t, self.tick, &mut s) {
                out.push((p.id, s));
            }
        }
    }

    /// A player's name (ours too, as the server took it).
    pub fn name(&self, id: u32) -> Option<&str> {
        if self.id == Some(id) { Some(&self.name) } else { self.peers.iter().find(|p| p.id == id).map(|p| p.name.as_str()) }
    }

    /// The other players, by id.
    pub fn peers(&self) -> impl Iterator<Item = u32> + '_ {
        self.peers.iter().map(|p| p.id)
    }

    /// A thing we do not hold, interpolated like the players, into a state that already exists (its
    /// joints keep their room: nothing is allocated). False (`out` untouched) if it is ours or nothing
    /// has come of it yet.
    pub fn rigid_into(&self, id: u64, now: f64, out: &mut RigidState) -> bool {
        match self.things.get(&id) {
            Some(slot) if !self.owns_thing(id) => slot.snaps.sample(self.drawn(now), self.tick, out),
            _ => false,
        }
    }

    /// A thing we do not hold as it must be *now*, not 100 ms ago: its newest snapshot carried to
    /// the present by its own velocity and spin (at most 250 ms). Returns how old that snapshot is,
    /// in seconds. This is what to steer a local simulation of the thing towards; `rigid_into` is
    /// what to draw when nothing simulates it. Its place is in the frame it was told in (`frame`).
    pub fn rigid_now(&self, id: u64, now: f64, out: &mut RigidState) -> Option<f32> {
        self.rigid_carried(id, now, EXTRAPOLATE, out)
    }

    /// `rigid_now`, carried on as far as `most` seconds: what to steer a copy toward that this
    /// game simulates as well (it goes on by itself meanwhile: a late word is still worth
    /// carrying all the way to now, not stopping short of it).
    pub fn rigid_carried(&self, id: u64, now: f64, most: f64, out: &mut RigidState) -> Option<f32> {
        let slot = self.things.get(&id).filter(|_| !self.owns_thing(id))?;
        let (stamp, state) = slot.snaps.newest()?;
        let age = (self.clock.server_time(now) - stamp).max(0.0);
        out.set(state);
        // (and as it was speeding up: a ship that turns or brakes is not carried straight on)
        let dt = age.min(most) as f32;
        out.carry(dt);
        if let Some((_, acc)) = slot.snaps.speeding(|s| s.vel) {
            let acc = acc.clamp_length_max(MAX_ACCEL);
            out.pos += (acc * (0.5 * dt * dt)).as_dvec3();
            out.vel += acc * dt;
        }
        Some(age as f32)
    }

    /// The moment (the server's clock, s) of the newest snapshot of a thing we do not hold.
    pub fn rigid_stamp(&self, id: u64) -> Option<f64> {
        self.things.get(&id).and_then(|slot| slot.snaps.newest()).map(|(stamp, _)| stamp)
    }

    /// Nothing more is kept of thing `id` (it is gone from the game).
    pub fn forget(&mut self, id: u64) {
        self.things.remove(&id);
        self.live.retain(|l| *l != id);
    }

    /// Tells the server we are leaving and stops. Nothing more is sent or received.
    pub fn close(&mut self) {
        if !matches!(self.phase, Phase::Over(_)) {
            let mut buf = [0u8; 64];
            let n = Datagram::Bye { salt: self.salt, reason: "" }.encode(&mut buf);
            // No answer is waited for: said three times, one gets there.
            for _ in 0..3 {
                self.transport.send(self.server, &buf[..n]);
            }
            self.phase = Phase::Over(text::BYE.to_string());
        }
    }

    /// The connection is over: the status says why, and if we were in, an event too.
    fn over(&mut self, reason: &str) {
        if matches!(self.phase, Phase::Live) {
            self.events.push(Event::Disconnected { reason: reason.to_string() });
        }
        self.phase = Phase::Over(reason.to_string());
    }

    /// The server's name, once connected.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
    /// Whether the server has told us what it knows (`Event::Synced` has come).
    pub fn synced(&self) -> bool {
        self.synced
    }
    /// How far in the past the others are drawn, seconds.
    pub fn delay(&self) -> f32 {
        self.delay.seconds() as f32
    }
    /// Whether our estimate of the server's clock is past its first pings (it only slides now).
    pub fn clock_settled(&self) -> bool {
        self.clock.settled()
    }

    /// The server's clock when ours says `now` (`None` until it has been measured).
    pub fn server_time(&self, now: f64) -> Option<f64> {
        self.clock.known().then(|| self.clock.server_time(now))
    }
    /// Times a second states are sent.
    pub fn tick_hz(&self) -> f32 {
        (1.0 / self.tick) as f32
    }
    /// Counters of the connection (bytes and datagrams each way, resends).
    pub fn stats(&self) -> ChannelStats {
        self.channel.stats()
    }
    /// Reliable messages (or their pieces) still on their way to the server.
    pub fn backlog(&self) -> usize {
        self.channel.backlog()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.close();
    }
}
