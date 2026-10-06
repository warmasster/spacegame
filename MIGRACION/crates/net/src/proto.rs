//! The protocol: what a datagram is (by its first byte) and the messages that travel inside the
//! channel. Everything is encoded with `wire`; decoding borrows from the datagram and allocates
//! nothing; anything that is not exactly a message is an `Err`.
//!
//! A client says `Hello`; the server answers with a `Challenge` (a number only that address can
//! have been told); the client says `Hello` again with it, and the server answers `Welcome` (or
//! `Refused`, with the reason). From then on both talk through a `Channel` (`Data` datagrams).
//! `Bye` ends it from either side.
//!
//! What the game says goes as bytes the server never reads: states in batches (`states`), things
//! told to everyone (`Tell`, `Hint`) or to one player (`To`). The server only knows who is in,
//! who the host is and who holds each key (`Claim`): the game can say new things without the
//! server changing.
pub mod states;

use crate::wire::{Reader, Wire, WireError, Writer};

/// First bytes of a `Hello`: what tells our datagrams from anything else that reaches the port.
pub const MAGIC: u32 = u32::from_le_bytes(*b"LUNA");
/// The protocol's version: both sides must have the same. 1 was V35's (the server read the game's
/// states); 2 is the relay that reads nothing.
pub const VERSION: u16 = 2;
/// The port a server listens on unless told otherwise (UDP).
pub const DEFAULT_PORT: u16 = 47600;

/// Caps of the strings on the wire, in bytes.
pub const MAX_NAME: usize = 96;
pub const MAX_BUILD: usize = 64;
pub const MAX_TEXT: usize = 1000;

/// The first byte of every datagram.
pub mod lead {
    pub const HELLO: u8 = 1;
    pub const WELCOME: u8 = 2;
    pub const REFUSED: u8 = 3;
    pub const DATA: u8 = 4;
    pub const BYE: u8 = 5;
    pub const CHALLENGE: u8 = 6;
}

/// Bytes of every `Hello` (it is padded to this). No answer to a hello is longer, so the server
/// cannot be used to throw at someone else more than was thrown at it.
pub const HELLO_SIZE: usize = 300;

/// A whole datagram. `salt` is a number the client makes up for each connection: the answers
/// carry it back, so a late answer to an earlier attempt is not taken for this one.
#[derive(Debug, PartialEq)]
pub enum Datagram<'a> {
    /// A client asks to come in. `cookie`: what the server's `Challenge` said (0 the first time).
    /// `scenario`: a number that says what world its game starts with (whoever comes with another
    /// is not let in). One of another version only has `version` and `salt` filled.
    Hello { version: u16, salt: u32, cookie: u64, scenario: u32, build: &'a str, name: &'a str },
    /// The server, to a hello without the right cookie: "say it again with this", to know the address is the sender's own.
    Challenge { salt: u32, cookie: u64 },
    /// The server lets it in. `name`: the client's name as the server took it (cleaned, made unique).
    Welcome { salt: u32, id: u32, tick_hz: u8, name: &'a str, server: &'a str },
    /// The server does not, and why.
    Refused { salt: u32, reason: &'a str },
    /// Either side ends the connection.
    Bye { salt: u32, reason: &'a str },
    /// A channel's datagram: what follows the lead byte.
    Data(&'a [u8]),
}

impl<'a> Datagram<'a> {
    /// Writes the datagram (not `Data`: the channel writes those) and returns its length.
    pub fn encode(&self, buf: &mut [u8]) -> usize {
        let mut w = Writer::new(buf);
        match *self {
            Datagram::Hello { version, salt, cookie, scenario, build, name } => {
                w.u8(lead::HELLO);
                w.u32(MAGIC);
                w.u16(version);
                w.u32(salt);
                w.u64(cookie);
                w.var(scenario as u64);
                w.str(build);
                w.str(name);
                let pad = [0u8; HELLO_SIZE];
                w.bytes(&pad[..HELLO_SIZE.saturating_sub(w.len())]);
            }
            Datagram::Challenge { salt, cookie } => {
                w.u8(lead::CHALLENGE);
                w.u32(salt);
                w.u64(cookie);
            }
            Datagram::Welcome { salt, id, tick_hz, name, server } => {
                w.u8(lead::WELCOME);
                w.u32(salt);
                w.var(id as u64);
                w.u8(tick_hz);
                w.str(name);
                w.str(server);
            }
            Datagram::Refused { salt, reason } => {
                w.u8(lead::REFUSED);
                w.u32(salt);
                w.str(reason);
            }
            Datagram::Bye { salt, reason } => {
                w.u8(lead::BYE);
                w.u32(salt);
                w.str(reason);
            }
            Datagram::Data(body) => {
                w.u8(lead::DATA);
                w.bytes(body);
            }
        }
        w.finish().unwrap_or(0)
    }

    pub fn decode(bytes: &'a [u8]) -> Wire<Datagram<'a>> {
        let mut r = Reader::new(bytes);
        let d = match r.u8()? {
            lead::HELLO => {
                if r.u32()? != MAGIC {
                    return Err(WireError::Value);
                }
                let (version, salt) = (r.u16()?, r.u32()?);
                if bytes.len() < HELLO_SIZE {
                    return Err(WireError::Short);
                }
                if version != VERSION {
                    // Whatever follows is another protocol's: all we can do is say so.
                    return Ok(Datagram::Hello { version, salt, cookie: 0, scenario: 0, build: "", name: "" });
                }
                // What follows the name is padding.
                return Ok(Datagram::Hello { version, salt, cookie: r.u64()?, scenario: r.var32()?, build: r.str(MAX_BUILD)?, name: r.str(MAX_NAME)? });
            }
            lead::CHALLENGE => Datagram::Challenge { salt: r.u32()?, cookie: r.u64()? },
            lead::WELCOME => Datagram::Welcome { salt: r.u32()?, id: r.var32()?, tick_hz: r.u8()?, name: r.str(MAX_NAME)?, server: r.str(MAX_NAME)? },
            lead::REFUSED => Datagram::Refused { salt: r.u32()?, reason: r.str(MAX_TEXT)? },
            lead::BYE => Datagram::Bye { salt: r.u32()?, reason: r.str(MAX_TEXT)? },
            lead::DATA => return Ok(Datagram::Data(r.rest())),
            _ => return Err(WireError::Value),
        };
        if r.is_empty() { Ok(d) } else { Err(WireError::Long) }
    }
}

mod tag {
    pub const PING: u8 = 1;
    pub const PONG: u8 = 2;
    pub const UP: u8 = 3;
    pub const DOWN: u8 = 4;
    pub const TELL: u8 = 5;
    pub const TOLD: u8 = 6;
    pub const HINT: u8 = 7;
    pub const HINTED: u8 = 8;
    pub const TO: u8 = 9;
    pub const FROM: u8 = 10;
    pub const CLAIM: u8 = 11;
    pub const RELEASE: u8 = 12;
    pub const OWNER: u8 = 13;
    pub const HOST: u8 = 14;
    pub const CHAT: u8 = 15;
    pub const SAID: u8 = 16;
    pub const JOINED: u8 = 17;
    pub const LEFT: u8 = 18;
    pub const SYNCED: u8 = 19;
    pub const BUNDLE: u8 = 20;
}

/// Bytes a message's own framing may take before what it carries (tag, a player, flags).
pub const FRAMING: usize = 16;

/// A message inside the channel. Times are microseconds. `data` is the game's: the server passes
/// it on as it came.
#[derive(Debug, PartialEq)]
pub enum Msg<'a> {
    /// Unreliable, client: "my clock says `t`".
    Ping { t: u64 },
    /// Unreliable, server: "you said `t`; mine says `server`".
    Pong { t: u64, server: u64 },
    /// Unreliable, client: its own states (its player, the things it holds): a `states` batch.
    Up(&'a [u8]),
    /// Unreliable, server: the states of the others, as it passes them on: a `states` batch.
    Down(&'a [u8]),
    /// Reliable, client: something for everyone else, in order (and back to itself in its place among
    /// what everyone told, with `echo`).
    Tell { echo: bool, data: &'a [u8] },
    /// Reliable, server: what player `by` told.
    Told { by: u32, data: &'a [u8] },
    /// Unreliable, client: something for everyone else that may be lost (a passing effect).
    Hint(&'a [u8]),
    /// Unreliable, server: what player `by` hinted.
    Hinted { by: u32, data: &'a [u8] },
    /// Reliable, client: something for one player alone.
    To { player: u32, data: &'a [u8] },
    /// Reliable, server: what player `by` sent to this client alone.
    From { by: u32, data: &'a [u8] },
    /// Reliable, client: it asks for a key (a thing to simulate, a seat). Keys are held in the order asked.
    Claim { key: u64 },
    /// Reliable, client: it gives a key up (or stops waiting for it).
    Release { key: u64 },
    /// Reliable, server: who holds a key now among those who asked (`None`: nobody asks for it any more).
    Owner { key: u64, player: Option<u32> },
    /// Reliable, server: who the host is (what nobody claims is theirs).
    Host { player: Option<u32> },
    /// Reliable, client: a chat line.
    Chat { text: &'a str },
    /// Reliable, server: a chat line; `from` is `None` when the server itself speaks.
    Said { from: Option<u32>, text: &'a str },
    /// Reliable, server: someone is in the game.
    Joined { id: u32, name: &'a str },
    /// Reliable, server: someone is gone.
    Left { id: u32 },
    /// Reliable, server: everything the server knows of the game has been told to this newcomer.
    Synced,
    /// Reliable, server: several reliable messages in one (each with its length before it): how a newcomer is
    /// told what the server knows without a hundred tiny messages. Read it with `Bundle`.
    Bundle(&'a [u8]),
}

/// The messages inside a `Msg::Bundle`, one after another.
pub struct Bundle<'a>(Reader<'a>);

impl<'a> Bundle<'a> {
    pub fn new(body: &'a [u8]) -> Bundle<'a> {
        Bundle(Reader::new(body))
    }
    /// A bundle to be filled with `push` and sent as it is (it is a whole encoded `Msg::Bundle`).
    pub fn begin() -> Vec<u8> {
        vec![tag::BUNDLE]
    }
    /// Adds an encoded message to a bundle.
    pub fn push(bundle: &mut Vec<u8>, msg: &[u8]) {
        let mut len = [0u8; 10];
        let mut w = Writer::new(&mut len);
        w.var(msg.len() as u64);
        let n = w.len();
        bundle.extend_from_slice(&len[..n]);
        bundle.extend_from_slice(msg);
    }
}

impl<'a> Iterator for Bundle<'a> {
    type Item = Wire<&'a [u8]>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        let item = self.0.var().and_then(|n| if n > self.0.left() as u64 { Err(WireError::Short) } else { self.0.bytes(n as usize) });
        if item.is_err() {
            self.0.rest(); // What follows a broken length cannot be read: stop here.
        }
        Some(item)
    }
}

fn opt(w: &mut Writer, id: Option<u32>) {
    w.var(id.map_or(0, |i| i as u64 + 1));
}

fn read_opt(r: &mut Reader) -> Wire<Option<u32>> {
    let v = r.var()?;
    if v == 0 { Ok(None) } else { u32::try_from(v - 1).map(Some).map_err(|_| WireError::Value) }
}

impl<'a> Msg<'a> {
    /// Whether this kind of message travels reliably: one that arrives the other way is not ours.
    pub fn reliable(&self) -> bool {
        !matches!(self, Msg::Ping { .. } | Msg::Pong { .. } | Msg::Up(_) | Msg::Down(_) | Msg::Hint(_) | Msg::Hinted { .. })
    }

    pub fn encode(&self, w: &mut Writer) {
        match *self {
            Msg::Ping { t } => {
                w.u8(tag::PING);
                w.var(t);
            }
            Msg::Pong { t, server } => {
                w.u8(tag::PONG);
                w.var(t);
                w.var(server);
            }
            Msg::Up(body) => {
                w.u8(tag::UP);
                w.bytes(body);
            }
            Msg::Down(body) => {
                w.u8(tag::DOWN);
                w.bytes(body);
            }
            Msg::Tell { echo, data } => {
                w.u8(tag::TELL);
                w.u8(u8::from(echo));
                w.bytes(data);
            }
            Msg::Told { by, data } => {
                w.u8(tag::TOLD);
                w.var(by as u64);
                w.bytes(data);
            }
            Msg::Hint(data) => {
                w.u8(tag::HINT);
                w.bytes(data);
            }
            Msg::Hinted { by, data } => {
                w.u8(tag::HINTED);
                w.var(by as u64);
                w.bytes(data);
            }
            Msg::To { player, data } => {
                w.u8(tag::TO);
                w.var(player as u64);
                w.bytes(data);
            }
            Msg::From { by, data } => {
                w.u8(tag::FROM);
                w.var(by as u64);
                w.bytes(data);
            }
            Msg::Claim { key } => {
                w.u8(tag::CLAIM);
                w.var(key);
            }
            Msg::Release { key } => {
                w.u8(tag::RELEASE);
                w.var(key);
            }
            Msg::Owner { key, player } => {
                w.u8(tag::OWNER);
                w.var(key);
                opt(w, player);
            }
            Msg::Host { player } => {
                w.u8(tag::HOST);
                opt(w, player);
            }
            Msg::Chat { text } => {
                w.u8(tag::CHAT);
                w.str(text);
            }
            Msg::Said { from, text } => {
                w.u8(tag::SAID);
                opt(w, from);
                w.str(text);
            }
            Msg::Joined { id, name } => {
                w.u8(tag::JOINED);
                w.var(id as u64);
                w.str(name);
            }
            Msg::Left { id } => {
                w.u8(tag::LEFT);
                w.var(id as u64);
            }
            Msg::Synced => w.u8(tag::SYNCED),
            Msg::Bundle(body) => {
                w.u8(tag::BUNDLE);
                w.bytes(body);
            }
        }
    }

    pub fn decode(bytes: &'a [u8]) -> Wire<Msg<'a>> {
        let mut r = Reader::new(bytes);
        let m = match r.u8()? {
            tag::PING => Msg::Ping { t: r.var()? },
            tag::PONG => Msg::Pong { t: r.var()?, server: r.var()? },
            tag::UP => Msg::Up(r.rest()),
            tag::DOWN => Msg::Down(r.rest()),
            tag::TELL => Msg::Tell {
                echo: match r.u8()? {
                    0 => false,
                    1 => true,
                    _ => return Err(WireError::Value),
                },
                data: r.rest(),
            },
            tag::TOLD => Msg::Told { by: r.var32()?, data: r.rest() },
            tag::HINT => Msg::Hint(r.rest()),
            tag::HINTED => Msg::Hinted { by: r.var32()?, data: r.rest() },
            tag::TO => Msg::To { player: r.var32()?, data: r.rest() },
            tag::FROM => Msg::From { by: r.var32()?, data: r.rest() },
            tag::CLAIM => Msg::Claim { key: r.var()? },
            tag::RELEASE => Msg::Release { key: r.var()? },
            tag::OWNER => Msg::Owner { key: r.var()?, player: read_opt(&mut r)? },
            tag::HOST => Msg::Host { player: read_opt(&mut r)? },
            tag::CHAT => Msg::Chat { text: r.str(MAX_TEXT)? },
            tag::SAID => Msg::Said { from: read_opt(&mut r)?, text: r.str(MAX_TEXT)? },
            tag::JOINED => Msg::Joined { id: r.var32()?, name: r.str(MAX_NAME)? },
            tag::LEFT => Msg::Left { id: r.var32()? },
            tag::SYNCED => Msg::Synced,
            tag::BUNDLE => Msg::Bundle(r.rest()),
            _ => return Err(WireError::Value),
        };
        if r.is_empty() { Ok(m) } else { Err(WireError::Long) }
    }
}
