//! Multiplayer: the wire, the transport, the session and what the game says over them.
//!
//! The connections of a server that has the game (`server`: who comes in, who goes, and what each
//! player's game and the server's say to each other, carried unread) and of each player
//! (`client`). What the game says is the game's own business (`lunar_play::net`): commands up,
//! snapshots and what happened down.
//! - `wire`, `quant`: bytes in and out, and the quantised values (angles, fixed point, rotations);
//! - `transport`: datagrams: real UDP, or an in-memory network that misbehaves on purpose (tests);
//! - `channel`: over datagrams, reliable ordered and unreliable sequenced messages, acks, RTT;
//! - `proto`: what a datagram and a message are;
//! - `game`: a player, a rigid thing: plain data, their encoding and their mixing;
//! - `clock`: one clock for all (the process's, and the server's as a client measures it);
//! - `seal`: each session sealed (keys agreed in the handshake, every datagram encrypted and signed);
//! - `text`: every text a person reads (Spanish) and the cleaning of what people type.
pub mod channel;
pub mod client;
pub mod clock;
pub mod game;
pub mod proto;
pub mod quant;
pub mod seal;
pub mod server;
pub mod text;
pub mod transport;
pub mod wire;

pub use channel::{Channel, ChannelError, ChannelStats, Inbox};
pub use client::{Client, Event, MAX_HINT, MAX_TELL, Status};
pub use clock::now;
pub use game::{Frame, PlayerState, RigidState, flag};
pub use proto::DEFAULT_PORT;
pub use server::{GameIn, PlayerInfo, Server, ServerConfig, ServerEvent, ServerStats};
pub use transport::{Addr, Conditions, MTU, Memory, MemoryNet, Transport, Udp};
pub use wire::{Reader, WireError, Writer};
