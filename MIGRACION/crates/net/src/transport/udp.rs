//! The real network: one non-blocking UDP socket.
use super::{Addr, Transport};
use std::io::ErrorKind;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

pub struct Udp {
    socket: UdpSocket,
}

impl Udp {
    /// Binds `addr` (`"0.0.0.0:47600"` for a server, `"0.0.0.0:0"` for a client: any free port).
    pub fn bind(addr: &str) -> std::io::Result<Udp> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Udp { socket })
    }
    /// A socket on any free port of the same family as `peer`, to talk to it.
    pub fn toward(peer: SocketAddr) -> std::io::Result<Udp> {
        Udp::bind(if peer.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" })
    }
    /// The address we are bound to (the port the system chose, when we asked for 0).
    pub fn local(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
    /// `"host:port"` to an address. A name is looked up (this can take a moment); a numeric address
    /// is immediate. Of the addresses of a name, an IPv4 one is preferred: that is what a server
    /// listens on unless told otherwise, and `localhost` often gives the IPv6 one first.
    pub fn resolve(addr: &str) -> std::io::Result<SocketAddr> {
        let all: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();
        all.iter().find(|a| a.is_ipv4()).or(all.first()).copied().ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, "no address"))
    }
}

impl Transport for Udp {
    fn send(&mut self, to: Addr, data: &[u8]) {
        if let Addr::Udp(a) = to {
            // A full send buffer or an unreachable host is a lost datagram: the session copes.
            let _ = self.socket.send_to(data, a);
        }
    }
    fn recv(&mut self, buf: &mut [u8]) -> Option<(Addr, usize)> {
        // Windows reports here, as an error, that an earlier datagram of ours reached a closed
        // port (one error per such datagram): those are not "nothing waiting", so skip a few.
        for _ in 0..64 {
            match self.socket.recv_from(buf) {
                Ok((n, from)) => return Some((Addr::Udp(from), n)),
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => return None,
                Err(_) => continue,
            }
        }
        None
    }
}
