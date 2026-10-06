//! What comes in from the server: the answer to our hello, then the channel's messages: the
//! others' states into their snapshot buffers, and events into the list the game drains.
use super::{Client, Event, HULL, JOINTS, MAX_THINGS, Peer, Phase};
use crate::channel::{Channel, ChannelError};
use crate::clock::seconds;
use crate::game::{PlayerState, RigidState, key};
use crate::proto::states::{DownReader, Source};
use crate::proto::{Bundle, Datagram, Msg, lead};
use crate::snap::SnapBuffer;
use crate::text;
use crate::wire::Reader;

impl Client {
    pub(super) fn receive(&mut self, now: f64) {
        let mut buf = std::mem::take(&mut self.datagram);
        while let Some((from, n)) = self.transport.recv(&mut buf) {
            if from != self.server {
                continue;
            }
            match Datagram::decode(&buf[..n]) {
                Ok(Datagram::Welcome { salt, id, tick_hz, name, server }) if salt == self.salt && matches!(self.phase, Phase::Hello) => {
                    self.phase = Phase::Live;
                    self.id = Some(id);
                    self.name = name.to_string();
                    self.server_name = server.to_string();
                    self.tick = 1.0 / tick_hz.clamp(1, 120) as f64;
                    self.channel = Channel::new(now, lead::DATA);
                    (self.next_send, self.next_ping) = (now, now);
                }
                Ok(Datagram::Challenge { salt, cookie }) if salt == self.salt && matches!(self.phase, Phase::Hello) => {
                    // The server wants to hear the hello again with this: at once, not at the next turn.
                    if self.cookie != cookie {
                        self.cookie = cookie;
                        self.last_hello = f64::NEG_INFINITY;
                    }
                }
                Ok(Datagram::Refused { salt, reason }) if salt == self.salt && matches!(self.phase, Phase::Hello) => self.phase = Phase::Over(reason.to_string()),
                Ok(Datagram::Bye { salt, reason }) if salt == self.salt => self.over(if reason.is_empty() { text::CLOSED } else { reason }),
                Ok(Datagram::Data(body)) if matches!(self.phase, Phase::Live) => {
                    let mut inbox = std::mem::take(&mut self.inbox);
                    inbox.clear();
                    let taken = self.channel.receive(body, now, &mut inbox);
                    for (reliable, msg) in inbox.iter() {
                        self.message(reliable, msg, now);
                    }
                    self.inbox = inbox;
                    if matches!(taken, Err(ChannelError::Window | ChannelError::TooLong)) {
                        self.over(text::SERVER_BROKEN);
                    }
                }
                _ => {}
            }
            if matches!(self.phase, Phase::Over(_)) {
                break;
            }
        }
        self.datagram = buf;
    }

    fn message(&mut self, reliable: bool, bytes: &[u8], now: f64) {
        let msg = match Msg::decode(bytes) {
            Ok(m) if m.reliable() == reliable => m,
            _ => return,
        };
        match msg {
            Msg::Pong { t, server } => self.clock.sample(seconds(t), seconds(server), now),
            Msg::Down(body) => self.others(body, now),
            Msg::Bundle(body) => {
                for inner in Bundle::new(body).flatten() {
                    if let Ok(m) = Msg::decode(inner) {
                        self.event(m);
                    }
                }
            }
            other => self.event(other),
        }
    }

    /// The states of the others, each into the buffer of its player or thing.
    fn others(&mut self, body: &[u8], now: f64) {
        let Ok(mut down) = DownReader::new(body) else { return };
        let server_now = self.clock.server_time(now);
        let mut rigid = std::mem::take(&mut self.rigid);
        while let Ok(Some(entry)) = down.next() {
            let stamp = seconds(entry.stamp);
            let mut r = Reader::new(entry.raw);
            // The first snapshot of something may be an old one, told to us for being new here:
            // only those that follow say how long the way is.
            let known = match entry.source {
                Source::Player(id) => {
                    let Ok(state) = PlayerState::decode(&mut r) else { continue };
                    if entry.sub != 0 || !r.is_empty() {
                        continue;
                    }
                    self.peers.iter_mut().find(|p| p.id == id).and_then(|p| {
                        let known = !p.snaps.is_empty();
                        p.snaps.push(stamp, entry.held).map(|slot| {
                            *slot = state;
                            known
                        })
                    })
                }
                Source::Thing(k) => {
                    let Some(id) = key::thing_of(k) else { continue };
                    if self.owns_thing(id) || (self.things.len() >= MAX_THINGS && !self.things.contains_key(&id)) {
                        continue;
                    }
                    let s = self.things.entry(id).or_default();
                    match entry.sub {
                        JOINTS => {
                            let _ = RigidState::decode_joints(&mut r, &mut s.joints);
                            None
                        }
                        HULL => {
                            rigid.id = id;
                            // A hull in a batch never brings joints: one that does is not ours.
                            if rigid.decode_rigid(&mut r).is_err() || !rigid.joints.is_empty() || !r.is_empty() {
                                continue;
                            }
                            let known = !s.snaps.is_empty();
                            s.snaps.push(stamp, entry.held).map(|slot| {
                                slot.set_with(&rigid, &s.joints);
                                known
                            })
                        }
                        _ => None,
                    }
                }
            };
            if known == Some(true) && self.clock.known() {
                self.delay.observe(server_now - stamp);
            }
        }
        self.rigid = rigid;
    }

    /// A thing passes to us or leaves us: what we had of it, either way, is of no use now.
    fn changed_hands(&mut self, id: u64) {
        if let Some(slot) = self.things.get_mut(&id) {
            slot.snaps.clear();
            slot.throttle.reset();
            slot.joints_throttle.reset();
            slot.set = false;
        }
    }

    fn event(&mut self, msg: Msg) {
        match msg {
            Msg::Joined { id, name } => {
                if Some(id) != self.id && !self.peers.iter().any(|p| p.id == id) {
                    self.peers.push(Peer { id, name: name.to_string(), snaps: SnapBuffer::default() });
                    self.events.push(Event::Joined { id, name: name.to_string() });
                }
            }
            Msg::Left { id } => {
                if let Some(at) = self.peers.iter().position(|p| p.id == id) {
                    let peer = self.peers.remove(at);
                    self.events.push(Event::Left { id, name: peer.name });
                }
            }
            Msg::Host { player } => {
                if player != self.host {
                    // Every thing nobody asks for changes hands: those that pass to us or leave us start anew.
                    let (me, was) = (self.id, self.host);
                    let moved: Vec<u64> = self.things.keys().copied().filter(|id| !self.holders.contains_key(&key::thing(*id)) && (was == me) != (player == me)).collect();
                    self.host = player;
                    moved.into_iter().for_each(|id| self.changed_hands(id));
                    self.events.push(Event::Host { player });
                }
            }
            Msg::Owner { key, player } => {
                let before = self.owner(key);
                match player {
                    Some(p) => {
                        self.holders.insert(key, p);
                    }
                    None => {
                        self.holders.remove(&key);
                    }
                }
                let after = self.owner(key);
                if let Some(id) = key::thing_of(key)
                    && (before == self.id) != (after == self.id)
                {
                    self.changed_hands(id);
                }
                self.events.push(Event::Owner { key, player });
            }
            Msg::Told { by, data } => self.events.push(Event::Told { by, data: data.to_vec() }),
            Msg::Hinted { by, data } => self.events.push(Event::Hinted { by, data: data.to_vec() }),
            Msg::From { by, data } => self.events.push(Event::Direct { by, data: data.to_vec() }),
            Msg::Said { from, text } => self.events.push(Event::Chat { from, text: text.to_string() }),
            Msg::Synced => {
                self.synced = true;
                self.events.push(Event::Synced);
            }
            _ => {}
        }
    }
}
