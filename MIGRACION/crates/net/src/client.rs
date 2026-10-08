//! A player's side of a game over a server that has the game: connects, says what its game
//! says to the game in the server (`send_game`, reliably and in order; `send_quick`, the newest
//! only) and hands on what that game says back (`Event::Game`), with who comes and goes and the
//! chat.
//! - `receive`: what comes in (the welcome, then the channel's messages);
//! - `send`: what goes out (the hello until it is answered, a ping now and then to know the
//!   server's clock).
//!
//! Use: `connect`, then every frame `update(now)` and drain `events()`. The same clock, in
//! seconds, for every `now` (`lunar_net::now()` is one).
mod receive;
mod send;

use crate::channel::{Channel, ChannelStats, Inbox, MAX_MESSAGE, MAX_UNRELIABLE};
use crate::clock::ServerClock;
use crate::proto::{Datagram, FRAMING, MAX_BUILD, Msg, lead};
use crate::text;
use crate::transport::{Addr, Transport, Udp};
use crate::wire::Writer;
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
    /// What the game that runs in the server said to us: reliably and in order (`Msg::Game`), or
    /// not (`Msg::Quick`: the newest only, the late ones dropped).
    Game { reliable: bool, data: Vec<u8> },
    /// A chat line (ours too, so everyone reads them in the same order); `None`: the server speaks.
    Chat { from: Option<u32>, text: String },
    /// The server has told us who is here.
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
}

/// Seconds between two hellos, and how long we keep saying hello.
const HELLO_EVERY: f64 = 0.25;
const HELLO_FOR: f64 = 5.0;
/// Seconds of silence from the server after which the connection is given up.
const TIMEOUT: f64 = 10.0;

/// The most bytes `send_game` takes in one go, and `send_quick`.
pub const MAX_TELL: usize = MAX_MESSAGE - FRAMING;
pub const MAX_HINT: usize = MAX_UNRELIABLE - FRAMING;

pub struct Client {
    transport: Box<dyn Transport>,
    server: Addr,
    phase: Phase,
    salt: u32,
    /// What the server's challenge said: the hello that is listened to carries it.
    cookie: u64,
    /// Our secret for this handshake (its key goes in the hello; the session is sealed with what
    /// it and the server's work out: `seal`).
    secret: crate::seal::Secret,
    name: String,
    build: String,
    scenario: u32,
    started: Option<f64>,
    last_hello: f64,
    channel: Channel,
    id: Option<u32>,
    server_name: String,
    synced: bool,
    clock: ServerClock,
    next_ping: f64,
    last_update: f64,
    peers: Vec<Peer>,
    events: Vec<Event>,
    // Buffers that go round.
    inbox: Inbox,
    datagram: Vec<u8>,
    msg: Vec<u8>,
}

impl Client {
    /// Starts connecting to `addr` (`"host:port"`) over UDP and returns at once: `status()` says
    /// `Connecting` until the server answers. `name`: the player's; `build`: the game's version
    /// (it must be the server's game's); `scenario`: a number that says what world the game starts
    /// with (the fingerprint of its data: it must be the server's).
    /// `Err` only if the address makes no sense or no socket can be opened.
    pub fn connect(addr: &str, name: &str, build: &str, scenario: u32) -> Result<Client, String> {
        let server = Udp::resolve(addr).map_err(|_| text::bad_address(addr))?;
        let udp = Udp::toward(server).map_err(|e| text::no_socket(&e.to_string()))?;
        Ok(Client::with_transport(Box::new(udp), Addr::Udp(server), name, build, scenario))
    }

    /// The same over any transport (tests and the server in the process use the in-memory one).
    pub fn with_transport(t: Box<dyn Transport>, server: Addr, name: &str, build: &str, scenario: u32) -> Client {
        // A number of our own for this connection; the hasher's seed is the system's randomness.
        let salt = RandomState::new().hash_one(std::process::id()) as u32;
        // (from the system's randomness; were there none, from the hasher's, as the salt)
        let secret = crate::seal::Secret::new().unwrap_or_else(|| {
            let mut b = [0u8; crate::seal::KEY];
            for (k, c) in b.chunks_mut(8).enumerate() {
                c.copy_from_slice(&RandomState::new().hash_one((std::process::id(), k)).to_le_bytes());
            }
            crate::seal::Secret::from_bytes(b)
        });
        Client {
            transport: t,
            server,
            phase: Phase::Hello,
            salt,
            cookie: 0,
            secret,
            name: text::clean_name(name),
            build: text::cut(build, MAX_BUILD).to_string(),
            scenario,
            started: None,
            last_hello: f64::NEG_INFINITY,
            channel: Channel::new(0.0, lead::DATA),
            id: None,
            server_name: String::new(),
            synced: false,
            clock: ServerClock::new(),
            next_ping: 0.0,
            last_update: 0.0,
            peers: Vec::new(),
            events: Vec::new(),
            inbox: Inbox::new(),
            datagram: vec![0; 2048],
            msg: Vec::new(),
        }
    }

    /// Call every frame: receives, fills the events, and sends what is due.
    pub fn update(&mut self, now: f64) {
        let dt = now - self.last_update;
        self.last_update = now;
        if matches!(self.phase, Phase::Over(_)) {
            return;
        }
        self.receive(now);
        match self.phase {
            Phase::Hello => self.hello(now),
            Phase::Live => {
                self.clock.advance(dt);
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

    /// Something for the game that runs in the server, reliably and in order: what our player
    /// does (`Msg::Game`). False if it could not be queued (not connected, too long).
    pub fn send_game(&mut self, data: &[u8]) -> bool {
        data.len() <= MAX_TELL && self.say(&Msg::Game(data), data.len())
    }

    /// The same, unreliable and sequenced (`Msg::Quick`): what is said again soon anyway (what we
    /// ask of each step, repeated).
    pub fn send_quick(&mut self, data: &[u8]) {
        if !matches!(self.phase, Phase::Live) || data.len() > MAX_HINT {
            return;
        }
        self.msg.resize(data.len() + FRAMING, 0);
        let mut w = Writer::new(&mut self.msg);
        Msg::Quick(data).encode(&mut w);
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

    /// A player's name (ours too, as the server took it).
    pub fn name(&self, id: u32) -> Option<&str> {
        if self.id == Some(id) { Some(&self.name) } else { self.peers.iter().find(|p| p.id == id).map(|p| p.name.as_str()) }
    }

    /// The other players, by id.
    pub fn peers(&self) -> impl Iterator<Item = u32> + '_ {
        self.peers.iter().map(|p| p.id)
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
    /// Whether the server has told us who is here (`Event::Synced` has come).
    pub fn synced(&self) -> bool {
        self.synced
    }
    /// Whether our estimate of the server's clock is past its first pings (it only slides now).
    pub fn clock_settled(&self) -> bool {
        self.clock.settled()
    }
    /// The server's clock when ours says `now` (`None` until it has been measured).
    pub fn server_time(&self, now: f64) -> Option<f64> {
        self.clock.known().then(|| self.clock.server_time(now))
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
