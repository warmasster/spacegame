//! The server's connections, with no IO of its own: it is handed the time and a transport, takes
//! players in and out, and carries what each player's game says to the game that runs in the
//! server (`take_game`) and what that game says back (`send_game`), unread. It also passes on the
//! chat and says who comes and goes.
//! - `join`: who may come in, and telling a newcomer who is here;
//! - `intake`: what the clients send (for the game, pings, chat);
//! - `session`: one connected player.
//!
//! Nothing that comes from the wire is trusted: sizes are capped, texts cleaned, and every
//! datagram of a session is sealed with its keys (`seal`): what does not open is dropped unread, and
//! a session whose datagrams start coming from another address (a router that changed it) is
//! known by them and follows them there.
mod intake;
mod join;
mod session;

use crate::channel::{ChannelError, ChannelStats, Inbox, MAX_MESSAGE};
use crate::proto::{Datagram, FRAMING, Msg};
use crate::text;
use crate::transport::{Addr, Transport};
use crate::wire::Writer;
use session::{Leaving, Session};
use std::hash::RandomState;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// The server's name, shown to who joins.
    pub name: String,
    pub max_players: usize,
    /// Seconds without hearing a client after which it is dropped.
    pub timeout: f64,
    /// A server that has the game itself: the build and the scenario (the fingerprint of its
    /// data) it plays. A hello of any other is refused, whoever comes first. None: the first to
    /// come says what game this is.
    pub game: Option<(String, u32)>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { name: "Servidor de Selene".to_string(), max_players: 16, timeout: 10.0, game: None }
    }
}

/// What happened, for whoever runs the server to tell (the library prints nothing).
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    /// Someone came in. `players`: how many are connected, this one counted.
    Joined { id: u32, name: String, addr: Addr, players: usize },
    /// Someone is gone, and why. `players`: how many remain.
    Left { id: u32, name: String, reason: String, players: usize },
    /// Someone was not let in, and why.
    Refused { addr: Addr, name: String, reason: String },
    /// Someone's datagrams, signed as theirs, come from another address now (their router
    /// changed it): they go on from there.
    Moved { id: u32, name: String, addr: Addr },
    /// A chat line.
    Chat { id: u32, name: String, text: String },
    /// The last player left.
    Empty,
}

/// Counters since the server was made.
#[derive(Clone, Copy, Debug, Default)]
pub struct ServerStats {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub datagrams_in: u64,
    pub datagrams_out: u64,
    /// Datagrams and messages that made no sense (and were dropped).
    pub garbled: u64,
    /// Datagrams from addresses that are not connected.
    pub strays: u64,
    /// Datagrams of a session whose signature did not pass (dropped unread).
    pub forged: u64,
    /// Messages for the game dropped for coming faster than the game takes them.
    pub flooded: u64,
}

/// Something a client said to the game that runs in the server (`Msg::Game`, `Msg::Quick`).
#[derive(Clone, Debug, PartialEq)]
pub struct GameIn {
    pub from: u32,
    /// It came reliably (and in order), or not.
    pub reliable: bool,
    pub data: Vec<u8>,
}

/// A connected player, for the console.
#[derive(Clone, Copy, Debug)]
pub struct PlayerInfo<'a> {
    pub id: u32,
    pub name: &'a str,
    pub addr: Addr,
    pub ping_ms: f32,
    /// Counters of this player's connection.
    pub channel: ChannelStats,
}

/// Encodes `msg` into `buf` (made as long as it needs) and gives its bytes.
fn pack<'b>(msg: &Msg, room: usize, buf: &'b mut Vec<u8>) -> &'b [u8] {
    buf.resize(room + FRAMING, 0);
    let mut w = Writer::new(buf);
    msg.encode(&mut w);
    let n = w.finish().unwrap_or(0);
    &buf[..n]
}

/// Room for the server's own small messages (names, texts).
const SMALL: usize = 2048;

/// The transport, counting what goes out through it.
struct Counting<'a> {
    inner: &'a mut dyn Transport,
    bytes: u64,
    datagrams: u64,
}

impl Transport for Counting<'_> {
    fn send(&mut self, to: Addr, data: &[u8]) {
        self.bytes += data.len() as u64;
        self.datagrams += 1;
        self.inner.send(to, data);
    }
    fn recv(&mut self, buf: &mut [u8]) -> Option<(Addr, usize)> {
        self.inner.recv(buf)
    }
}

/// What a hello says.
struct Hello<'a> {
    version: u16,
    salt: u32,
    cookie: u64,
    key: [u8; crate::seal::KEY],
    scenario: u32,
    build: &'a str,
    name: &'a str,
}

/// Refusals said in any one second: more hellos than this that cannot come in are not answered
/// (nor logged), so a flood of them costs the server and its log next to nothing.
const REFUSALS_A_SECOND: u32 = 8;
/// Datagrams from unknown addresses checked against the sessions' keys in any one second.
const STRAYS_CHECKED_A_SECOND: u32 = 256;

pub struct Server {
    config: ServerConfig,
    /// In the order they came, which is the order of their ids.
    sessions: Vec<Session>,
    /// The game's build and the number of its scenario, as the first to come said them (with no
    /// `ServerConfig::game`).
    playing: (String, u32),
    next_id: u32,
    events: Vec<ServerEvent>,
    stats: ServerStats,
    /// The key of the cookies of this run: what proves a hello comes from the address it says.
    secret: RandomState,
    /// The second we are counting refusals in, and how many so far; and the same for datagrams
    /// from addresses we do not know that were checked against every session's key.
    refused: (f64, u32),
    strays_checked: (f64, u32),
    // Buffers that go round.
    inbox: Inbox,
    datagram: Vec<u8>,
    msg: Vec<u8>,
    /// What clients said to the game, not taken yet.
    game_in: Vec<GameIn>,
}

impl Server {
    pub fn new(config: ServerConfig) -> Server {
        let config = ServerConfig { max_players: config.max_players.max(1), timeout: config.timeout.max(1.0), ..config };
        Server {
            config,
            sessions: Vec::new(),
            playing: (String::new(), 0),
            next_id: 1,
            events: Vec::new(),
            stats: ServerStats::default(),
            secret: RandomState::new(),
            refused: (0.0, 0),
            strays_checked: (0.0, 0),
            inbox: Inbox::new(),
            datagram: vec![0; 2048],
            msg: Vec::new(),
            game_in: Vec::new(),
        }
    }

    /// Takes what has arrived, drops who is gone and sends what each client is owed. `now`: seconds
    /// of a clock that never goes back (`lunar_net::now()`). The more often, the sooner things go.
    pub fn update(&mut self, now: f64, transport: &mut dyn Transport) {
        let mut t = Counting { inner: transport, bytes: 0, datagrams: 0 };
        let mut buf = std::mem::take(&mut self.datagram);
        while let Some((from, n)) = t.recv(&mut buf) {
            self.stats.datagrams_in += 1;
            self.stats.bytes_in += n as u64;
            self.take(from, &buf[..n], now, &mut t);
        }
        self.datagram = buf;
        self.reap(now, &mut t);
        for s in self.sessions.iter_mut().filter(|s| s.confirmed) {
            s.channel.flush(now, s.addr, &mut t);
        }
        self.stats.bytes_out += t.bytes;
        self.stats.datagrams_out += t.datagrams;
    }

    fn find(&self, addr: Addr) -> Option<usize> {
        self.sessions.iter().position(|s| s.addr == addr)
    }

    fn take(&mut self, from: Addr, bytes: &[u8], now: f64, t: &mut dyn Transport) {
        match Datagram::decode(bytes) {
            Ok(Datagram::Data(body)) => {
                let Some(i) = self.find(from).or_else(|| self.moved(from, body, now)) else {
                    self.stats.strays += 1;
                    return;
                };
                let mut inbox = std::mem::take(&mut self.inbox);
                inbox.clear();
                match self.sessions[i].channel.receive(body, now, &mut inbox) {
                    Ok(()) => self.sessions[i].confirmed = true,
                    Err(ChannelError::Wire(_)) => self.stats.garbled += 1,
                    // (not theirs: it changes nothing of theirs)
                    Err(ChannelError::Forged) => self.stats.forged += 1,
                    Err(_) => self.expel(i, text::BROKEN, Some(text::BROKEN)),
                }
                for (reliable, msg) in inbox.iter() {
                    self.message(i, reliable, msg, now);
                }
                self.inbox = inbox;
            }
            Ok(Datagram::Hello { version, salt, cookie, key, scenario, build, name }) => self.hello(from, Hello { version, salt, cookie, key, scenario, build, name }, now, t),
            Ok(Datagram::Bye { salt, .. }) => {
                if let Some(i) = self.find(from).filter(|i| self.sessions[*i].salt == salt) {
                    self.drop_session(i, Leaving { reason: text::LEFT.to_string(), bye: None }, t);
                }
            }
            _ => self.stats.garbled += 1,
        }
    }

    /// The session a datagram from an address we do not know is signed by (its router gave it
    /// another address): it follows it there. A few hundred such a second are looked into at most,
    /// so a flood of them costs next to nothing.
    fn moved(&mut self, from: Addr, body: &[u8], now: f64) -> Option<usize> {
        if now - self.strays_checked.0 >= 1.0 {
            self.strays_checked = (now, 0);
        }
        if self.strays_checked.1 >= STRAYS_CHECKED_A_SECOND || body.len() < crate::channel::HEADER {
            return None;
        }
        self.strays_checked.1 += 1;
        let i = self.sessions.iter().position(|s| s.confirmed && s.leaving.is_none() && s.channel.opens(body))?;
        let s = &mut self.sessions[i];
        s.addr = from;
        self.events.push(ServerEvent::Moved { id: s.id, name: s.name.clone(), addr: from });
        Some(i)
    }

    /// Marks session `i` to be dropped at the end of this update.
    fn expel(&mut self, i: usize, reason: &str, bye: Option<&str>) {
        let s = &mut self.sessions[i];
        if s.leaving.is_none() {
            s.leaving = Some(Leaving { reason: reason.to_string(), bye: bye.map(str::to_string) });
        }
    }

    /// Drops whoever was marked to go and whoever has been silent too long.
    fn reap(&mut self, now: f64, t: &mut dyn Transport) {
        let mut i = 0;
        while i < self.sessions.len() {
            let s = &mut self.sessions[i];
            let silent = s.channel.silence(now) > self.config.timeout;
            match s.leaving.take().or_else(|| silent.then(|| Leaving { reason: text::SILENT.to_string(), bye: Some(text::LOST.to_string()) })) {
                Some(leaving) => self.drop_session(i, leaving, t),
                None => i += 1,
            }
        }
    }

    fn drop_session(&mut self, i: usize, leaving: Leaving, t: &mut dyn Transport) {
        let s = self.sessions.remove(i);
        if let Some(bye) = &leaving.bye {
            let mut buf = [0u8; 1100];
            let n = Datagram::Bye { salt: s.salt, reason: bye }.encode(&mut buf);
            t.send(s.addr, &buf[..n]);
        }
        self.events.push(ServerEvent::Left { id: s.id, name: s.name, reason: leaving.reason, players: self.sessions.len() });
        if self.sessions.is_empty() {
            self.playing = (String::new(), 0);
            self.events.push(ServerEvent::Empty);
            return;
        }
        self.send_all(&Msg::Left { id: s.id }, None);
    }

    /// Sends a small reliable message of the server's own to everyone but `except`.
    fn send_all(&mut self, msg: &Msg, except: Option<u32>) {
        let mut buf = std::mem::take(&mut self.msg);
        let bytes = pack(msg, SMALL, &mut buf);
        self.send_packed(bytes, except);
        self.msg = buf;
    }

    fn send_packed(&mut self, bytes: &[u8], except: Option<u32>) {
        if bytes.is_empty() || bytes.len() > MAX_MESSAGE {
            return;
        }
        for i in 0..self.sessions.len() {
            if Some(self.sessions[i].id) != except && !self.sessions[i].channel.send_reliable(bytes) {
                self.expel(i, text::BEHIND, Some(text::BEHIND));
            }
        }
    }

    /// What clients said to the game that runs in the server since the last call, in the order it
    /// came (each one's reliable messages in the order they were sent).
    pub fn take_game(&mut self) -> std::vec::Drain<'_, GameIn> {
        for s in &mut self.sessions {
            s.game_in = 0;
        }
        self.game_in.drain(..)
    }

    /// Says `data` to player `to` for its game, reliably (`Msg::Game`, at most `MAX_TELL` bytes) or
    /// not (`Msg::Quick`, at most `MAX_HINT`). False if there is no such player, it is too long
    /// (nothing goes), or (reliably) the player is too far behind to take more: it is dropped at
    /// the next update.
    pub fn send_game(&mut self, to: u32, reliable: bool, data: &[u8]) -> bool {
        if data.len() > if reliable { crate::client::MAX_TELL } else { crate::client::MAX_HINT } {
            return false;
        }
        let Some(i) = self.sessions.iter().position(|s| s.id == to && s.confirmed) else { return false };
        let mut buf = std::mem::take(&mut self.msg);
        let msg = if reliable { Msg::Game(data) } else { Msg::Quick(data) };
        let bytes = pack(&msg, data.len(), &mut buf);
        let sent = if bytes.is_empty() {
            false
        } else if reliable {
            let ok = bytes.len() <= MAX_MESSAGE && self.sessions[i].channel.send_reliable(bytes);
            if !ok {
                self.expel(i, text::BEHIND, Some(text::BEHIND));
            }
            ok
        } else {
            bytes.len() <= crate::channel::MAX_UNRELIABLE && {
                self.sessions[i].channel.send_unreliable(bytes);
                true
            }
        };
        self.msg = buf;
        sent
    }

    /// How long a datagram takes to player `id` and back (s), as its channel measures it.
    pub fn rtt(&self, id: u32) -> Option<f32> {
        self.sessions.iter().find(|s| s.id == id).map(|s| s.channel.rtt())
    }

    /// What happened since the last call.
    pub fn events(&mut self) -> std::vec::Drain<'_, ServerEvent> {
        self.events.drain(..)
    }
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }
    pub fn stats(&self) -> ServerStats {
        self.stats
    }
    pub fn player_count(&self) -> usize {
        self.sessions.len()
    }
    /// The connected players, in the order they came.
    pub fn players(&self) -> impl Iterator<Item = PlayerInfo<'_>> {
        self.sessions.iter().map(|s| PlayerInfo { id: s.id, name: &s.name, addr: s.addr, ping_ms: s.channel.rtt() * 1000.0, channel: s.channel.stats() })
    }
    /// Throws a player out at the next update, telling them why (`reason` may be empty). False if there is no such player.
    pub fn kick(&mut self, id: u32, reason: &str) -> bool {
        let Some(i) = self.sessions.iter().position(|s| s.id == id) else { return false };
        let reason = text::clean(reason, text::CHAT_CHARS - 40);
        let told = if reason.is_empty() { text::KICKED.to_string() } else { format!("{}: {reason}", text::KICKED) };
        self.expel(i, text::KICKED, Some(&told));
        true
    }
    /// Says something to everyone as the server.
    pub fn say(&mut self, text: &str) {
        let text = text::clean(text, text::CHAT_CHARS);
        if !text.is_empty() {
            self.send_all(&Msg::Said { from: None, text: &text }, None);
        }
    }
    /// Says goodbye to everyone and forgets them: the server is closing.
    pub fn close(&mut self, transport: &mut dyn Transport) {
        while !self.sessions.is_empty() {
            self.drop_session(0, Leaving { reason: text::CLOSED.to_string(), bye: Some(text::CLOSED.to_string()) }, transport);
        }
    }
}
