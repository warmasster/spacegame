//! The states of everyone to everyone else, once a tick: for each client, one batch (as many
//! messages as it takes, each fitting a datagram) with the states that came since its last batch,
//! as their bytes came. A client never gets its own player nor the things it tells of itself; a
//! newcomer gets everything there is. Things nobody has told of for a while are forgotten: they
//! are at rest.
use super::{KEEP, Server};
use crate::channel::MAX_UNRELIABLE;
use crate::clock::micros;
use crate::proto::states::{DownWriter, Whose};
use crate::wire::Writer;

fn stage(staged: &mut Vec<u8>, msg: &[u8]) {
    staged.extend_from_slice(&(msg.len() as u16).to_le_bytes());
    staged.extend_from_slice(msg);
}

impl Server {
    pub(super) fn relay(&mut self, now: f64) {
        let base = micros(now);
        let mut msg = std::mem::take(&mut self.msg);
        let mut staged = std::mem::take(&mut self.stage);
        msg.resize(MAX_UNRELIABLE, 0);
        for r in 0..self.sessions.len() {
            // Whoever has not answered the welcome yet gets nothing: it will get everything when it does.
            if !self.sessions[r].confirmed {
                continue;
            }
            let (me, since) = (self.sessions[r].id, self.sessions[r].relayed);
            let players = self.sessions.iter().filter(|s| s.id != me).flat_map(|s| s.own.iter().enumerate().map(move |(sub, st)| (Whose::Own, sub as u8, s.id, st)));
            // (a thing's streams from the highest down: its joints before its hull, so whoever reads
            // has them when the hull's state comes)
            let things = self.world.things.iter().filter(|(_, t)| t.from != me && t.tick > since).flat_map(|(key, t)| t.subs.iter().enumerate().rev().map(move |(sub, st)| (Whose::Thing(*key), sub as u8, 0, st)));
            staged.clear();
            let mut w = Writer::new(&mut msg);
            let mut down = DownWriter::begin(&mut w, base);
            let mut any = false;
            for (whose, sub, player, state) in players.chain(things).filter(|e| e.3.tick > since) {
                if !down.entry(&mut w, whose, sub, player, state.stamp, state.held, &state.bytes) {
                    // The message is full: it goes as it is, and another one starts.
                    let len = w.len();
                    stage(&mut staged, &msg[..len]);
                    w = Writer::new(&mut msg);
                    down = DownWriter::begin(&mut w, base);
                    down.entry(&mut w, whose, sub, player, state.stamp, state.held, &state.bytes);
                }
                any = true;
            }
            if any {
                let len = w.len();
                stage(&mut staged, &msg[..len]);
            }
            let session = &mut self.sessions[r];
            let mut at = 0;
            while at < staged.len() {
                let len = u16::from_le_bytes([staged[at], staged[at + 1]]) as usize;
                session.channel.send_unreliable(&staged[at + 2..at + 2 + len]);
                at += 2 + len;
            }
            session.relayed = self.tick;
        }
        // what nobody tells of any more is at rest (now and then: it is a look at every thing)
        let keep = (KEEP * self.config.tick_hz as f64) as u64;
        if self.tick % 32 == 0 && self.tick > keep {
            // (never what some client has not been sent yet)
            let behind = self.sessions.iter().filter(|s| s.confirmed).map(|s| s.relayed).min().unwrap_or(self.tick);
            self.world.prune((self.tick - keep).min(behind));
        }
        self.tick += 1;
        self.msg = msg;
        self.stage = staged;
    }
}
