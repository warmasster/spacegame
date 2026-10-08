//! What goes out to the server besides what the game says: the hello until it answers, and a
//! ping now and then to know its clock (and to be heard while the game says nothing).
use super::{Client, HELLO_EVERY, HELLO_FOR, Phase};
use crate::clock::micros;
use crate::proto::{Datagram, HELLO_SIZE, Msg, VERSION};
use crate::text;
use crate::wire::Writer;

/// The first pings come close together, to know the server's clock soon (until enough answers
/// came back: some are lost, or overtaken and dropped); then one every so often keeps it.
const QUICK_EVERY: f64 = 0.15;
const PING_EVERY: f64 = 2.0;

impl Client {
    pub(super) fn hello(&mut self, now: f64) {
        let started = *self.started.get_or_insert(now);
        if now - started > HELLO_FOR {
            self.phase = Phase::Over(text::NO_ANSWER.to_string());
        } else if now - self.last_hello >= HELLO_EVERY {
            self.last_hello = now;
            let mut buf = [0u8; HELLO_SIZE];
            let n = Datagram::Hello { version: VERSION, salt: self.salt, cookie: self.cookie, scenario: self.scenario, build: &self.build, name: &self.name }.encode(&mut buf);
            self.transport.send(self.server, &buf[..n]);
        }
    }

    pub(super) fn send(&mut self, now: f64) {
        if now >= self.next_ping {
            self.next_ping = now + if self.clock.settled() { PING_EVERY } else { QUICK_EVERY };
            let mut buf = [0u8; 16];
            let mut w = Writer::new(&mut buf);
            Msg::Ping { t: micros(now) }.encode(&mut w);
            let n = w.finish().unwrap_or(0);
            self.channel.send_unreliable(&buf[..n]);
        }
    }
}
