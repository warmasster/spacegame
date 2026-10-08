//! A session sealed: whoever sees its traffic reads nothing of it, and whoever is not in it puts
//! nothing in it. In the handshake each side shows a key of its own made for that handshake
//! (X25519: the client in its `Hello`, the server in its `Welcome`); from what both work out of
//! them and what the handshake said (the server's cookie, the client's salt) come two keys, one
//! for each way (HKDF with SHA-256); every datagram of the channel then goes encrypted and signed
//! with the key of its way and a number of its own, never the same twice (ChaCha20-Poly1305), the
//! lead byte signed with it. One that does not open is dropped before anything of it is read.
//!
//! What it does not stop: someone in the middle of the way from the very start, who answers the
//! hello as the server (it would take the server's own key, known to the player beforehand).
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit, Nonce, Tag};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

/// Bytes a sealed datagram has besides what it carries: its number, after the lead byte, and its
/// seal, at the end.
pub const NUMBER: usize = 8;
pub const SEAL: usize = 16;
pub const OVERHEAD: usize = NUMBER + SEAL;
/// Bytes of a key shown in a handshake.
pub const KEY: usize = 32;

/// One side's secret for one handshake, and the key it shows of it.
pub struct Secret {
    secret: StaticSecret,
    pub public: [u8; KEY],
}

impl Secret {
    /// A new one, from the system's randomness (none if the system gives none).
    pub fn new() -> Option<Secret> {
        let mut b = [0u8; KEY];
        getrandom::fill(&mut b).ok()?;
        Some(Secret::from_bytes(b))
    }

    /// One from these bytes (tests, and whoever brings their own randomness).
    pub fn from_bytes(b: [u8; KEY]) -> Secret {
        let secret = StaticSecret::from(b);
        let public = PublicKey::from(&secret).to_bytes();
        Secret { secret, public }
    }
}

/// The keys of a session, one for each way; the number of the next datagram sent; and the newest
/// number received with the 64 before it, as bits (one that came already is not taken again).
pub struct Keys {
    send: ChaCha20Poly1305,
    recv: ChaCha20Poly1305,
    next: u64,
    newest: Option<u64>,
    seen: u64,
}

impl Keys {
    /// What `mine` and the other side's `theirs` agree on, with what the handshake said (`cookie`,
    /// `salt`), as the client sees it or as the server does. None if `theirs` is not a key anyone
    /// could agree on anything with (a point that gives nothing: someone in the way).
    pub fn agree(mine: &Secret, theirs: &[u8; KEY], cookie: u64, salt: u32, client: bool) -> Option<Keys> {
        let shared = mine.secret.diffie_hellman(&PublicKey::from(*theirs));
        if !shared.was_contributory() {
            return None;
        }
        let mut said = [0u8; 12];
        said[..8].copy_from_slice(&cookie.to_le_bytes());
        said[8..].copy_from_slice(&salt.to_le_bytes());
        let mut okm = [0u8; 64];
        Hkdf::<Sha256>::new(Some(&said), shared.as_bytes()).expand(b"luna: sesion sellada v1", &mut okm).ok()?;
        let (to_server, to_client) = (ChaCha20Poly1305::new_from_slice(&okm[..32]).ok()?, ChaCha20Poly1305::new_from_slice(&okm[32..]).ok()?);
        let (send, recv) = if client { (to_server, to_client) } else { (to_client, to_server) };
        Some(Keys { send, recv, next: 0, newest: None, seen: 0 })
    }

    fn nonce(n: u64) -> Nonce {
        let mut b = [0u8; 12];
        b[4..].copy_from_slice(&n.to_le_bytes());
        Nonce::from(b)
    }

    /// The datagram in `buf[..len]` sealed where it is: `buf[0]` its lead byte (signed, not
    /// hidden), `buf[1..1 + NUMBER]` left for its number, what it carries from there on. Its seal
    /// goes after it (`buf` has room for `SEAL` more): the length of it sealed.
    pub fn seal(&mut self, buf: &mut [u8], len: usize) -> usize {
        let n = self.next;
        self.next += 1;
        buf[1..1 + NUMBER].copy_from_slice(&n.to_le_bytes());
        let (head, rest) = buf.split_at_mut(1 + NUMBER);
        let Ok(tag) = self.send.encrypt_in_place_detached(&Self::nonce(n), &head[..1], &mut rest[..len - 1 - NUMBER]) else {
            return 0;
        };
        buf[len..len + SEAL].copy_from_slice(&tag);
        len + SEAL
    }

    /// What datagram `lead` + `body` (what follows its lead byte) carries, opened into `out`:
    /// `Err(false)` if it was not sealed with this session's key or was changed on the way,
    /// `Err(true)` if it came already (a network that says some twice, or someone who says it
    /// again: nothing new either way).
    pub fn open<'a>(&mut self, lead: u8, body: &[u8], out: &'a mut [u8]) -> Result<&'a [u8], bool> {
        let n = u64::from_le_bytes(body.get(..NUMBER).and_then(|b| b.try_into().ok()).ok_or(false)?);
        let opened = self.opened(n, lead, body, out).ok_or(false)?;
        if self.newest.is_some_and(|m| n <= m && (m - n >= 64 || self.seen & (1 << (m - n)) != 0)) {
            return Err(true);
        }
        match self.newest {
            Some(m) if n <= m => self.seen |= 1 << (m - n),
            Some(m) => {
                self.seen = if n - m >= 64 { 1 } else { (self.seen << (n - m)) | 1 };
                self.newest = Some(n);
            }
            None => (self.newest, self.seen) = (Some(n), 1),
        }
        Ok(opened)
    }

    /// Whether datagram `lead` + `body` is sealed with this session's key (nothing is taken of it).
    pub fn opens(&self, lead: u8, body: &[u8]) -> bool {
        let mut out = [0u8; crate::transport::MTU];
        body.get(..NUMBER).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).is_some_and(|n| self.opened(n, lead, body, &mut out).is_some())
    }

    fn opened<'a>(&self, n: u64, lead: u8, body: &[u8], out: &'a mut [u8]) -> Option<&'a [u8]> {
        if body.len() < OVERHEAD {
            return None;
        }
        let (data, tag) = body[NUMBER..].split_at(body.len() - OVERHEAD);
        let out = out.get_mut(..data.len())?;
        out.copy_from_slice(data);
        self.recv.decrypt_in_place_detached(&Self::nonce(n), &[lead], out, Tag::from_slice(tag)).ok()?;
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Keys, Keys) {
        let (a, b) = (Secret::from_bytes([7; KEY]), Secret::from_bytes([42; KEY]));
        (Keys::agree(&a, &b.public, 99, 5, true).unwrap(), Keys::agree(&b, &a.public, 99, 5, false).unwrap())
    }

    fn sealed(k: &mut Keys, lead: u8, data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 1 + NUMBER + data.len() + SEAL];
        buf[0] = lead;
        buf[1 + NUMBER..1 + NUMBER + data.len()].copy_from_slice(data);
        let n = k.seal(&mut buf, 1 + NUMBER + data.len());
        buf.truncate(n);
        buf
    }

    #[test]
    fn what_one_side_seals_only_the_other_opens_once_and_unchanged() {
        let (mut client, mut server) = pair();
        let mut out = [0u8; 1500];
        let d = sealed(&mut client, 4, b"lo que dice el jugador");
        // (hidden: what it carries is not there to be read)
        assert!(!d.windows(5).any(|w| w == b"jugad"));
        assert_eq!(server.open(4, &d[1..], &mut out), Ok(&b"lo que dice el jugador"[..]));
        // (not twice; not with the lead byte changed; not with a bit of it changed; not by whoever
        // sealed it, nor by a session of another handshake)
        assert_eq!(server.open(4, &d[1..], &mut out), Err(true), "taken twice");
        let d = sealed(&mut client, 4, b"otra cosa");
        assert!(server.opens(4, &d[1..]) && !server.opens(5, &d[1..]));
        let mut bad = d.clone();
        bad[12] ^= 1;
        assert!(!server.opens(4, &bad[1..]));
        assert!(!client.opens(4, &d[1..]), "the way back is another key");
        let (_, mut other) = {
            let (a, b) = (Secret::from_bytes([7; KEY]), Secret::from_bytes([43; KEY]));
            (Keys::agree(&a, &b.public, 99, 5, true).unwrap(), Keys::agree(&b, &a.public, 99, 5, false).unwrap())
        };
        assert_eq!(other.open(4, &d[1..], &mut out), Err(false));
        // (out of order, late but within the window: taken; too late: not)
        let late = sealed(&mut client, 4, b"tarde");
        let mut newer = Vec::new();
        for _ in 0..70 {
            newer = sealed(&mut client, 4, b"nuevo");
        }
        assert!(server.open(4, &newer[1..], &mut out).is_ok());
        assert_eq!(server.open(4, &late[1..], &mut out), Err(true), "taken from 70 back");
        // (the server's way, to the client)
        let back = sealed(&mut server, 4, b"respuesta");
        assert_eq!(client.open(4, &back[1..], &mut out), Ok(&b"respuesta"[..]));
    }

    #[test]
    fn a_key_that_gives_nothing_is_refused_and_each_handshake_has_its_own() {
        let a = Secret::from_bytes([7; KEY]);
        assert!(Keys::agree(&a, &[0; KEY], 1, 2, true).is_none(), "a point that gives nothing");
        assert_ne!(Secret::new().unwrap().public, Secret::new().unwrap().public);
    }

    #[test]
    fn sealing_and_opening_a_datagram_costs_little() {
        let (mut client, mut server) = pair();
        let data = [0x5au8; 1150];
        let mut out = [0u8; 1500];
        let mut buf = vec![0u8; 1200];
        let began = std::time::Instant::now();
        let n = 20_000;
        for _ in 0..n {
            buf[0] = 4;
            buf[1 + NUMBER..1 + NUMBER + data.len()].copy_from_slice(&data);
            let len = client.seal(&mut buf, 1 + NUMBER + data.len());
            assert!(server.open(4, &buf[1..len], &mut out).is_ok());
        }
        let each = began.elapsed().as_secs_f64() * 1e6 / f64::from(n);
        eprintln!("sellar y abrir un datagrama de 1 200 bytes: {each:.2} µs");
        assert!(each < 50.0, "{each:.1} µs");
    }
}
