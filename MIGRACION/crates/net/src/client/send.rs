//! What goes out to the server: the hello until it answers; then, at the tick rate, our player
//! and the things of ours that move (only what changed: see `throttle`), and a ping now and then
//! to know its clock.
use super::{Client, HELLO_EVERY, HELLO_FOR, HULL, JOINTS, Phase};
use crate::channel::MAX_UNRELIABLE;
use crate::clock::micros;
use crate::game::{PlayerState, key};
use crate::proto::states::{Whose, begin_up, up_entry};
use crate::proto::{Datagram, HELLO_SIZE, Msg, VERSION};
use crate::text;
use crate::throttle::{HEARTBEAT, NEVER};
use crate::wire::Writer;
use glam::{DVec3, Vec3};

/// The first pings come close together, to know the server's clock soon; then one every so often keeps it.
const QUICK_PINGS: u32 = 6;
const QUICK_EVERY: f64 = 0.15;
const PING_EVERY: f64 = 2.0;

fn encoded(buf: &mut [u8], write: impl FnOnce(&mut Writer)) -> usize {
    let mut w = Writer::new(buf);
    write(&mut w);
    w.finish().unwrap_or(0)
}

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
            self.pings += 1;
            self.next_ping = now + if self.pings < QUICK_PINGS { QUICK_EVERY } else { PING_EVERY };
            let mut buf = [0u8; 16];
            let n = encoded(&mut buf, |w| Msg::Ping { t: micros(now) }.encode(w));
            self.channel.send_unreliable(&buf[..n]);
        }
        // Our states carry the server's time: until it is known there is nothing to stamp them with.
        if now >= self.next_send && self.clock.known() {
            self.next_send = if now - self.next_send > self.tick { now + self.tick } else { self.next_send + self.tick };
            self.ours(now);
        }
    }

    /// One batch with what is due of ours, in as many messages as it takes.
    fn ours(&mut self, now: f64) {
        let stamp = micros(self.clock.server_time(self.sampled));
        let mut msg = std::mem::take(&mut self.msg);
        msg.resize(MAX_UNRELIABLE, 0);
        let mut raw = [0u8; 1024];
        let mut w = Writer::new(&mut msg);
        begin_up(&mut w, stamp);
        let mut any = false;
        if let Some(me) = &self.me {
            // While riding a ship, the world position and velocity change by themselves and the
            // others do not use them: they do not count as news.
            let key = if me.ride.is_some() { PlayerState { pos: DVec3::ZERO, vel: Vec3::ZERO, ..*me } } else { *me };
            let n = encoded(&mut raw, |w| key.encode(w));
            if let Some(held) = self.me_throttle.due(&raw[..n], now, HEARTBEAT) {
                let n = encoded(&mut raw, |w| me.encode(w));
                any |= up_entry(&mut w, Whose::Own, 0, held, &raw[..n]);
            }
        }
        let (me, host) = (self.id, self.host);
        let mut joints = [0u8; 700];
        let mut k = 0;
        while k < self.live.len() {
            let id = self.live[k];
            let ours = me.is_some() && self.holders.get(&key::thing(id)).copied().or(host) == me;
            let Some(slot) = self.things.get_mut(&id).filter(|_| ours) else {
                // Gone, or no longer ours: nothing of it to send.
                if let Some(slot) = self.things.get_mut(&id) {
                    slot.listed = false;
                }
                self.live.swap_remove(k);
                continue;
            };
            let set = std::mem::take(&mut slot.set);
            // News is any change, of the hull or of a joint; the joints themselves only go when it is they that changed.
            let n = encoded(&mut raw, |w| slot.mine.encode(w));
            let Some(held) = slot.throttle.due(&raw[..n], now, NEVER) else {
                // Nothing new of it and nobody gave it lately: it is at rest, and is not looked at again until it moves.
                slot.listed = set || slot.throttle.pending();
                if slot.listed {
                    k += 1;
                } else {
                    self.live.swap_remove(k);
                }
                continue;
            };
            k += 1;
            let j = encoded(&mut joints, |w| slot.mine.encode_joints(w));
            let j = if slot.mine.joints.is_empty() || slot.joints_throttle.due(&joints[..j], now, NEVER).is_none() { 0 } else { j };
            let n = encoded(&mut raw, |w| slot.mine.encode_rigid(w));
            let whose = Whose::Thing(key::thing(id));
            let put = |w: &mut Writer| (j == 0 || up_entry(w, whose, JOINTS, false, &joints[..j])) && up_entry(w, whose, HULL, held, &raw[..n]);
            let mark = w.mark();
            if !put(&mut w) {
                // The message is full: it goes without this thing, and another one starts with it.
                w.rewind(mark);
                let len = w.len();
                self.channel.send_unreliable(&msg[..len]);
                w = Writer::new(&mut msg);
                begin_up(&mut w, stamp);
                put(&mut w);
            }
            any = true;
        }
        let len = w.len();
        if any {
            self.channel.send_unreliable(&msg[..len]);
        }
        self.msg = msg;
    }
}
