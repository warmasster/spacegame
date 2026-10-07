//! What the clients send, once it has come out of their channel: their states are kept to be
//! passed on at the next tick; what they tell (to everyone, to one), the keys they ask for and
//! their chat lines are dealt with at once. Nothing of what the game says is read.
use super::world::Thing;
use super::{GameIn, Server, ServerEvent, pack};
use crate::channel::MAX_UNRELIABLE;
use crate::clock::micros;
use crate::proto::Msg;
use crate::proto::states::{SUBS, UpReader, Whose};
use crate::text;

/// What clients said to the game and it has not taken yet, at most (a game that stalls must not
/// make the server keep everything a flood of them says).
const MAX_GAME_IN: usize = 1 << 14;

/// A state stamped further back than this is taken as of this long ago (microseconds).
const OLDEST: u64 = 2_000_000;
/// A sender's idea of our clock may run a little ahead of it: a state stamped up to this far in
/// the future keeps its stamp (it is the sender's stamps, evenly spaced, that make the others
/// see it move evenly); further ahead it is taken as of now.
const AHEAD: u64 = 100_000;

impl Server {
    pub(super) fn message(&mut self, i: usize, reliable: bool, bytes: &[u8], now: f64) {
        let msg = match Msg::decode(bytes) {
            // A reliable kind that came unreliably (or the reverse) is not something our client sends.
            Ok(m) if m.reliable() == reliable => m,
            _ => {
                self.stats.garbled += 1;
                return;
            }
        };
        let id = self.sessions[i].id;
        match msg {
            Msg::Ping { t } => {
                let pong = pack(&Msg::Pong { t, server: micros(now) }, 32, &mut self.msg);
                self.sessions[i].channel.send_unreliable(pong);
            }
            Msg::Up(body) => self.states(i, body, now),
            Msg::Tell { echo, data } => {
                let mut buf = std::mem::take(&mut self.msg);
                let passed = pack(&Msg::Told { by: id, data }, data.len(), &mut buf);
                // (with `echo`, to the one who told it too: everyone gets what was told in one order)
                self.send_packed(passed, if echo { None } else { Some(id) });
                self.stats.told += 1;
                self.msg = buf;
            }
            Msg::Hint(data) => {
                let mut buf = std::mem::take(&mut self.msg);
                let passed = pack(&Msg::Hinted { by: id, data }, data.len(), &mut buf);
                if !passed.is_empty() && passed.len() <= MAX_UNRELIABLE {
                    for s in self.sessions.iter_mut().filter(|s| s.id != id && s.confirmed) {
                        s.channel.send_unreliable(passed);
                    }
                }
                self.msg = buf;
            }
            Msg::To { player, data } => {
                let mut buf = std::mem::take(&mut self.msg);
                let passed = pack(&Msg::From { by: id, data }, data.len(), &mut buf);
                if let Some(to) = self.sessions.iter().position(|s| s.id == player)
                    && !passed.is_empty()
                    && !self.sessions[to].channel.send_reliable(passed)
                {
                    self.expel(to, text::BEHIND, Some(text::BEHIND));
                }
                self.stats.told += 1;
                self.msg = buf;
            }
            Msg::Claim { key } => {
                if self.world.claim(id, key, self.config.max_keys) {
                    self.send_all(&Msg::Owner { key, player: Some(id) }, None);
                } else {
                    // Someone had it first (or it asks for too many): told who holds it, so it knows where it stands.
                    let held = pack(&Msg::Owner { key, player: self.world.holder(key) }, 32, &mut self.msg);
                    self.sessions[i].channel.send_reliable(held);
                }
            }
            Msg::Release { key } => {
                if self.world.release(id, key) {
                    self.send_all(&Msg::Owner { key, player: self.world.holder(key) }, None);
                }
            }
            // (for the game that runs in the server: kept until it takes it)
            Msg::Game(data) | Msg::Quick(data) => {
                if self.game_in.len() < MAX_GAME_IN {
                    self.game_in.push(GameIn { from: id, reliable, data: data.to_vec() });
                }
            }
            Msg::Chat { text } => {
                let text = text::clean(text, text::CHAT_CHARS);
                if !text.is_empty() {
                    // To the one who said it too: everyone reads the lines in the same order.
                    self.send_all(&Msg::Said { from: Some(id), text: &text }, None);
                    self.events.push(ServerEvent::Chat { id, name: self.sessions[i].name.clone(), text });
                }
            }
            _ => self.stats.garbled += 1,
        }
    }

    /// A client's own states: its own record, and the things whose key it holds (those of any other thing are ignored and counted).
    fn states(&mut self, i: usize, body: &[u8], now: f64) {
        let Ok(mut up) = UpReader::new(body) else {
            self.stats.garbled += 1;
            return;
        };
        // The moment is the sender's, but never far in the future nor long ago.
        let now_us = micros(now);
        let stamp = if up.stamp() > now_us + AHEAD { now_us } else { up.stamp().max(now_us.saturating_sub(OLDEST)) };
        let (id, tick, host) = (self.sessions[i].id, self.tick, self.host_now());
        loop {
            let entry = match up.next() {
                Ok(Some(e)) => e,
                Ok(None) => break,
                Err(_) => {
                    self.stats.garbled += 1;
                    break;
                }
            };
            let sub = usize::from(entry.sub).min(SUBS - 1);
            match entry.whose {
                Whose::Own => {
                    self.sessions[i].own[sub].take(entry.raw, stamp, entry.held, tick);
                }
                Whose::Thing(key) if self.world.owner(key, host) == Some(id) => {
                    if !self.world.things.contains_key(&key) && self.world.things.len() >= self.config.max_things {
                        continue;
                    }
                    let thing = self.world.things.entry(key).or_insert_with(Thing::default);
                    if thing.from != id {
                        // It changed hands: what its last holder said is of no use now.
                        *thing = Thing { from: id, ..Thing::default() };
                    }
                    if thing.subs[sub].take(entry.raw, stamp, entry.held, tick) {
                        thing.tick = tick;
                    }
                }
                Whose::Thing(_) => self.stats.foreign += 1,
            }
        }
    }
}
