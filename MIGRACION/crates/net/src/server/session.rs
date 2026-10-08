//! One connected player as the server sees them.
use crate::channel::Channel;
use crate::transport::Addr;

/// Why a session ends: what the log and the others are told, and what the player is told (if anything can be).
pub(super) struct Leaving {
    pub reason: String,
    pub bye: Option<String>,
}

pub(super) struct Session {
    pub id: u32,
    pub addr: Addr,
    /// The number the client made up for this connection, and the key we showed it (said again if
    /// our welcome is lost).
    pub salt: u32,
    pub key: [u8; crate::seal::KEY],
    pub name: String,
    pub channel: Channel,
    /// The client has answered our welcome through the channel: until then nothing is sent
    /// through it (it would arrive before the welcome, or instead of it, and be thrown away).
    pub confirmed: bool,
    pub leaving: Option<Leaving>,
    /// Messages for the game taken of it since the game last took them (`take_game`): past
    /// `MAX_GAME_IN_EACH` the rest are dropped, so one who floods does not crowd out the others.
    pub game_in: u32,
}
