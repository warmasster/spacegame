//! What comes in from the server: the answer to our hello, then the channel's messages: events
//! into the list the game drains.
use super::{Client, Event, Peer, Phase};
use crate::channel::{Channel, ChannelError};
use crate::clock::seconds;
use crate::proto::{Bundle, Datagram, Msg, lead};
use crate::text;

impl Client {
    pub(super) fn receive(&mut self, now: f64) {
        let mut buf = std::mem::take(&mut self.datagram);
        while let Some((from, n)) = self.transport.recv(&mut buf) {
            if from != self.server {
                continue;
            }
            match Datagram::decode(&buf[..n]) {
                Ok(Datagram::Welcome { salt, id, name, server }) if salt == self.salt && matches!(self.phase, Phase::Hello) => {
                    self.phase = Phase::Live;
                    self.id = Some(id);
                    self.name = name.to_string();
                    self.server_name = server.to_string();
                    self.channel = Channel::new(now, lead::DATA);
                    self.next_ping = now;
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

    fn event(&mut self, msg: Msg) {
        match msg {
            Msg::Joined { id, name } => {
                if Some(id) != self.id && !self.peers.iter().any(|p| p.id == id) {
                    self.peers.push(Peer { id, name: name.to_string() });
                    self.events.push(Event::Joined { id, name: name.to_string() });
                }
            }
            Msg::Left { id } => {
                if let Some(at) = self.peers.iter().position(|p| p.id == id) {
                    let peer = self.peers.remove(at);
                    self.events.push(Event::Left { id, name: peer.name });
                }
            }
            Msg::Said { from, text } => self.events.push(Event::Chat { from, text: text.to_string() }),
            Msg::Game(data) => self.events.push(Event::Game { reliable: true, data: data.to_vec() }),
            Msg::Quick(data) => self.events.push(Event::Game { reliable: false, data: data.to_vec() }),
            Msg::Synced => {
                self.synced = true;
                self.events.push(Event::Synced);
            }
            _ => {}
        }
    }
}
