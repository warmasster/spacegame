//! Who may come in, and telling a newcomer who is here. What the world is like is the game's to
//! tell (the game that runs in the server is told who came: `ServerEvent::Joined`).
use super::session::{Leaving, Session};
use super::{Hello, REFUSALS_A_SECOND, SMALL, Server, ServerEvent, pack};
use crate::channel::Channel;
use crate::proto::{Bundle, Datagram, HELLO_SIZE, Msg, VERSION, lead};
use crate::text;
use crate::transport::{Addr, Transport};
use std::hash::BuildHasher;

/// Bytes of each bundle of the catch-up (the channel cuts it in pieces of a datagram).
const BUNDLE: usize = 16 * 1024;

impl Server {
    pub(super) fn hello(&mut self, from: Addr, hello: Hello, now: f64, t: &mut dyn Transport) {
        let Hello { version, salt, cookie, scenario, build, name } = hello;
        if let Some(i) = self.find(from)
            && self.sessions[i].salt == salt
        {
            // Our welcome was lost on the way: say it again.
            self.welcome(i, t);
            return;
        }
        let name = text::clean_name(name);
        if version != VERSION {
            self.refuse(from, salt, name, text::version(VERSION, version), now, t);
            return;
        }
        // Anyone can write any address on a datagram. Only who really is at this one gets our
        // answer, and with it the number to say hello with: nothing is kept for those who are not.
        let expected = self.secret.hash_one((from, salt));
        if cookie != expected {
            let mut buf = [0u8; 16];
            let n = Datagram::Challenge { salt, cookie: expected }.encode(&mut buf);
            t.send(from, &buf[..n]);
            return;
        }
        if let Some(i) = self.find(from) {
            // The same address starts over (the game was closed and opened again): the old session is gone.
            self.drop_session(i, Leaving { reason: text::BACK.to_string(), bye: None }, t);
        }
        let refusal = if self.sessions.len() >= self.config.max_players {
            Some(text::full(self.config.max_players))
        } else if let Some((b, sc)) = &self.config.game {
            if build != b {
                Some(text::build(b, build))
            } else if scenario != *sc {
                Some(text::scenario(*sc, scenario))
            } else {
                None
            }
        } else if self.sessions.is_empty() {
            // The first to come says what game this is.
            None
        } else if build != self.playing.0 {
            Some(text::build(&self.playing.0, build))
        } else if scenario != self.playing.1 {
            Some(text::scenario(self.playing.1, scenario))
        } else {
            None
        };
        if let Some(reason) = refusal {
            self.refuse(from, salt, name, reason, now, t);
            return;
        }
        if self.sessions.is_empty() {
            self.playing = (build.to_string(), scenario);
        }
        let id = self.next_id;
        self.next_id += 1;
        // Two players with one name would be told apart by nobody.
        let name = if self.sessions.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) { format!("{} ({id})", text::clean(&name, text::NAME_CHARS - 6)) } else { name };
        self.sessions.push(Session { id, addr: from, salt, name: name.clone(), channel: Channel::new(now, lead::DATA), confirmed: false, leaving: None });
        let i = self.sessions.len() - 1;
        self.welcome(i, t);
        // The others learn of the newcomer; the newcomer is told who is here in one go.
        self.send_all(&Msg::Joined { id, name: &name }, Some(id));
        self.catch_up(i);
        self.events.push(ServerEvent::Joined { id, name, addr: from, players: self.sessions.len() });
    }

    fn refuse(&mut self, to: Addr, salt: u32, name: String, reason: String, now: f64, t: &mut dyn Transport) {
        if now - self.refused.0 >= 1.0 {
            self.refused = (now, 0);
        }
        self.refused.1 += 1;
        if self.refused.1 > REFUSALS_A_SECOND {
            self.stats.strays += 1;
            return;
        }
        let mut buf = [0u8; HELLO_SIZE];
        // Never longer than the hello it answers.
        let n = Datagram::Refused { salt, reason: &reason }.encode(&mut buf);
        if n > 0 {
            t.send(to, &buf[..n]);
        }
        self.events.push(ServerEvent::Refused { addr: to, name, reason });
    }

    fn welcome(&mut self, i: usize, t: &mut dyn Transport) {
        let s = &self.sessions[i];
        let mut buf = [0u8; 512];
        let n = Datagram::Welcome { salt: s.salt, id: s.id, name: &s.name, server: &text::clean(&self.config.name, text::NAME_CHARS) }.encode(&mut buf);
        t.send(s.addr, &buf[..n]);
    }

    /// Who is here, to a newcomer, in as few reliable messages as it takes.
    fn catch_up(&mut self, i: usize) {
        let me = self.sessions[i].id;
        let mut one = std::mem::take(&mut self.msg);
        let mut bundles: Vec<Vec<u8>> = Vec::new();
        let mut add = |msg: &[u8]| {
            if bundles.last().is_none_or(|b| b.len() + msg.len() + 3 > BUNDLE) {
                bundles.push(Bundle::begin());
            }
            if let Some(b) = bundles.last_mut() {
                Bundle::push(b, msg);
            }
        };
        for s in self.sessions.iter().filter(|s| s.id != me) {
            add(pack(&Msg::Joined { id: s.id, name: &s.name }, SMALL, &mut one));
        }
        add(pack(&Msg::Synced, SMALL, &mut one));
        self.msg = one;
        if !bundles.iter().all(|b| self.sessions[i].channel.send_reliable(b)) {
            self.expel(i, text::BEHIND, Some(text::BEHIND));
        }
    }
}
