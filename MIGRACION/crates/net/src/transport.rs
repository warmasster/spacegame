//! Datagrams in and out: the one thing the session needs from a network. Two of them: `Udp` (the
//! real one) and `Memory` (an in-process network for tests that can lose, delay, duplicate and
//! reorder on purpose, always the same way for the same seed).
mod memory;
mod udp;

pub use memory::{Conditions, Memory, MemoryNet};
pub use udp::Udp;

use std::fmt;
use std::net::SocketAddr;

/// The largest datagram we ever send: under the 1280 bytes every IPv6 path carries and the ~1400
/// that survive tunnels and home routers, with room for the IP and UDP headers.
pub const MTU: usize = 1200;

/// Where a datagram goes to or comes from: small, comparable, the same type for both transports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Addr {
    /// A host and port of the real network.
    Udp(SocketAddr),
    /// An endpoint of a `MemoryNet`.
    Mem(u32),
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Addr::Udp(a) => write!(f, "{a}"),
            Addr::Mem(n) => write!(f, "mem:{n}"),
        }
    }
}

/// Unreliable datagrams, never blocking.
pub trait Transport: Send {
    /// Sends `data` to `to`. Best effort: a datagram that cannot go is lost, as on the wire.
    fn send(&mut self, to: Addr, data: &[u8]);
    /// The next datagram waiting, copied into `buf` (one longer than `buf` is cut or dropped): who sent it
    /// and its length, or `None` when there is none right now.
    fn recv(&mut self, buf: &mut [u8]) -> Option<(Addr, usize)>;
}
