//! One connected player as the server sees them, and the last state it holds of something.
use crate::channel::Channel;
use crate::proto::states::SUBS;
use crate::transport::Addr;

/// The newest state received of a player or a thing, as its bytes came, to pass on as they are.
#[derive(Default)]
pub(super) struct Stored {
    pub bytes: Vec<u8>,
    /// The moment its sender sampled it, microseconds of our clock.
    pub stamp: u64,
    /// The relay tick in which it came; 0: nothing yet.
    pub tick: u64,
    /// See `throttle`: the thing stood still before this state.
    pub held: bool,
}

impl Stored {
    pub fn put(&mut self, raw: &[u8], stamp: u64, held: bool, tick: u64) {
        // Two states within one tick: only the second is passed on, and it must still say that the thing had been still.
        self.held = held || (self.held && self.tick == tick);
        self.bytes.clear();
        self.bytes.extend_from_slice(raw);
        (self.stamp, self.tick) = (stamp, tick);
    }
    /// Takes `raw` if it is not older than what is held; false if it was dropped for being older.
    pub fn take(&mut self, raw: &[u8], stamp: u64, held: bool, tick: u64) -> bool {
        if self.tick != 0 && stamp < self.stamp {
            return false;
        }
        self.put(raw, stamp, held, tick);
        true
    }
}

/// Why a session ends: what the log and the others are told, and what the player is told (if anything can be).
pub(super) struct Leaving {
    pub reason: String,
    pub bye: Option<String>,
}

pub(super) struct Session {
    pub id: u32,
    pub addr: Addr,
    /// The number the client made up for this connection.
    pub salt: u32,
    pub name: String,
    pub channel: Channel,
    /// The client has answered our welcome through the channel: until then nothing is sent
    /// through it (it would arrive before the welcome, or instead of it, and be thrown away).
    pub confirmed: bool,
    /// Its own states (its player), stream by stream.
    pub own: [Stored; SUBS],
    /// The relay tick up to which this client has been sent everything.
    pub relayed: u64,
    pub leaving: Option<Leaving>,
}
