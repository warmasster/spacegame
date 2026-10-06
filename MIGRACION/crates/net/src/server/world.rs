//! What the server knows of the game being played: how it started, who asked for which key (and
//! so who holds it), and the newest state of each thing that moves, to pass it on. No simulation
//! and nothing of what the states say: the clients have the world; the server only keeps what
//! makes them agree.
//!
//! The rule of the keys: a key is held by the first of those who asked for it and have not let it
//! go; one nobody asks for is the host's if it is a thing's (`game::key::hosted`), nobody's if not.
use super::session::Stored;
use crate::game::key;
use crate::proto::states::SUBS;
use std::collections::BTreeMap;

/// A thing somebody holds and tells of: its newest states, stream by stream.
#[derive(Default)]
pub(super) struct Thing {
    pub subs: [Stored; SUBS],
    /// Who sent them (they are never passed back to them).
    pub from: u32,
    /// The relay tick of the newest of them.
    pub tick: u64,
}

#[derive(Default)]
pub(super) struct World {
    /// The game's build and the number of its scenario, as the first client said them.
    pub build: String,
    pub scenario: u32,
    /// Things told of lately, by key (in order: the same batches on every run).
    pub things: BTreeMap<u64, Thing>,
    /// Who asked for each key, in the order they asked.
    claims: BTreeMap<u64, Vec<u32>>,
    /// Keys each player asks for, to cap them.
    asked: BTreeMap<u32, u32>,
}

impl World {
    /// A new game begins (the first player came): nothing of the last one remains.
    pub fn start(&mut self, build: &str, scenario: u32) {
        self.clear();
        self.build.push_str(build);
        self.scenario = scenario;
    }
    pub fn clear(&mut self) {
        self.build.clear();
        self.scenario = 0;
        self.things.clear();
        self.claims.clear();
        self.asked.clear();
    }
    /// Who holds `key` among those who asked for it.
    pub fn holder(&self, key: u64) -> Option<u32> {
        self.claims.get(&key).and_then(|c| c.first().copied())
    }
    /// Whose `key` is: its holder's, or the host's if it is a thing's and nobody asked for it.
    pub fn owner(&self, key: u64, host: Option<u32>) -> Option<u32> {
        self.holder(key).or(if key::hosted(key) { host } else { None })
    }
    /// How many keys are asked for.
    pub fn claimed(&self) -> usize {
        self.claims.len()
    }
    /// The keys asked for and who holds each.
    pub fn holders(&self) -> impl Iterator<Item = (u64, u32)> + '_ {
        self.claims.iter().filter_map(|(k, c)| c.first().map(|p| (*k, *p)))
    }
    /// `player` asks for `key` (at most `cap` keys each). True if that changes who holds it.
    pub fn claim(&mut self, player: u32, key: u64, cap: u32) -> bool {
        let asked = self.asked.entry(player).or_default();
        let list = self.claims.entry(key).or_default();
        if list.contains(&player) || *asked >= cap {
            if list.is_empty() {
                self.claims.remove(&key);
            }
            return false;
        }
        *asked += 1;
        list.push(player);
        list.len() == 1
    }
    /// `player` lets `key` go (or stops waiting for it). True if that changes who holds it.
    pub fn release(&mut self, player: u32, key: u64) -> bool {
        let Some(list) = self.claims.get_mut(&key) else { return false };
        let Some(at) = list.iter().position(|p| *p == player) else { return false };
        list.remove(at);
        if let Some(n) = self.asked.get_mut(&player) {
            *n = n.saturating_sub(1);
        }
        if list.is_empty() {
            self.claims.remove(&key);
        }
        at == 0
    }
    /// `player` is gone: everything they asked for is let go, and what they told of is forgotten.
    /// The keys whose holder changed are added to `changed`.
    pub fn leave(&mut self, player: u32, changed: &mut Vec<u64>) {
        for (key, list) in &mut self.claims {
            if let Some(at) = list.iter().position(|p| *p == player) {
                list.remove(at);
                if at == 0 {
                    changed.push(*key);
                }
            }
        }
        self.claims.retain(|_, list| !list.is_empty());
        self.asked.remove(&player);
        self.things.retain(|_, t| t.from != player);
    }
    /// Forgets the things not told of since tick `before`: they are at rest, and whoever needs to
    /// know where is told by the game itself.
    pub fn prune(&mut self, before: u64) {
        self.things.retain(|_, t| t.tick >= before);
    }
}
