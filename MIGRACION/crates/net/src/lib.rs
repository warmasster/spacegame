//! Multiplayer: the wire, the transport, the session and what the game says over them.
//!
//! A dedicated server that simulates nothing and reads nothing of what the game says (`server`)
//! and a client for each player (`client`). Every client simulates the whole world; the network
//! keeps them in agreement: each sends its own player and the things it holds (`claim`), the
//! server passes them on and decides who holds what, and whatever else the game has to say (a hand
//! on a control, a hit, a snapshot of a ship for whoever comes late) travels as bytes of the
//! game's own, to everyone or to one player, reliably or not.
//! - `wire`, `quant`: bytes in and out, and the quantised values (angles, fixed point, rotations);
//! - `transport`: datagrams: real UDP, or an in-memory network that misbehaves on purpose (tests);
//! - `channel`: over datagrams, reliable ordered and unreliable sequenced messages, acks, RTT;
//! - `proto`: what a datagram and a message are;
//! - `game`: a player, a rigid thing: plain data, their encoding and their mixing; the keys;
//! - `snap`, `throttle`, `clock`: snapshot interpolation, sending only what changed, one clock for all;
//! - `text`: every text a person reads (Spanish) and the cleaning of what people type.
pub mod channel;
pub mod client;
pub mod clock;
pub mod game;
pub mod proto;
pub mod quant;
pub mod server;
pub mod snap;
pub mod text;
pub mod throttle;
pub mod transport;
pub mod wire;

pub use channel::{Channel, ChannelError, ChannelStats, Inbox};
pub use client::{Client, Event, MAX_HINT, MAX_TELL, Status};
pub use clock::now;
pub use game::{Frame, PlayerState, RigidState, flag, key};
pub use proto::DEFAULT_PORT;
pub use server::{PlayerInfo, Server, ServerConfig, ServerEvent, ServerStats};
pub use transport::{Addr, Conditions, MTU, Memory, MemoryNet, Transport, Udp};
pub use wire::{Reader, WireError, Writer};
