//! A connection with one peer over datagrams. Every datagram starts with a small header that
//! numbers it and acknowledges the ones received (the last one and, bit by bit, the 32 before:
//! each ack travels in up to 33 datagrams, so losing some costs nothing). On top of that:
//! - **reliable ordered** messages: resent until a datagram that carried them is acked, handed
//!   over once and in order; one longer than a datagram is cut in pieces and joined again;
//! - **unreliable sequenced** messages: sent once; one that arrives after a newer datagram is
//!   dropped (the newest wins);
//! - several messages per datagram, datagrams of at most `MTU` bytes;
//! - round-trip time, keep-alives when there is nothing to say, and how long the peer has been silent;
//! - signed, once the session has a key (`sign`): every datagram ends with its SipHash, and one
//!   whose tag does not pass is dropped before anything of it is read (`ChannelError::Forged`).
//!
//! The channel does no IO of its own and allocates nothing once warm.
mod inbox;
mod incoming;
mod outgoing;
mod rtt;

pub use inbox::Inbox;

use crate::transport::{Addr, MTU, Transport};
use crate::wire::{Reader, WireError, Writer};
use incoming::Incoming;
use outgoing::{Carried, Outgoing};
use rtt::Rtt;

/// Bytes of the header of every datagram: lead (1), sequence (2), ack (2), ack bits (4), ack delay (1).
pub const HEADER: usize = 10;
/// Reliable items that may be in flight at once (the sender never goes further than this from
/// the oldest one not acked, so the receiver's slots always suffice).
pub const WINDOW: u16 = 256;
/// Bytes of each piece of a reliable message longer than this.
pub const FRAGMENT: usize = 1024;
/// The longest reliable message.
pub const MAX_MESSAGE: usize = 64 * 1024;
/// Bytes of the tag a signed datagram ends with.
pub const TAG: usize = 8;
/// The longest unreliable message: what fits one datagram after the header, its framing and the tag.
pub const MAX_UNRELIABLE: usize = MTU - HEADER - 3 - TAG;
/// Reliable items that may wait to be acked before `send_reliable` refuses: a peer this far behind is gone.
pub const MAX_BACKLOG: usize = 8192;
/// Bytes of unreliable messages that may wait for a flush.
const MAX_LOOSE: usize = 64 * 1024;
/// Datagrams one flush may send (the ack bits of the peer cover 33).
const MAX_BURST: usize = 16;
/// Seconds without sending anything after which an empty datagram goes, so the peer knows we are here.
pub const KEEPALIVE: f64 = 1.0;
/// Datagrams remembered to know what an ack acknowledges.
const SENT: usize = 256;

const TAG_LOOSE: u8 = 0;
const TAG_WHOLE: u8 = 1;
const TAG_MORE: u8 = 2;

/// Why a datagram could not be taken. `Wire`: it is cut or garbled (it is dropped; the channel goes on).
/// `Window` and `TooLong`: the peer does not keep to the protocol (the connection should be closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelError {
    /// The datagram is cut or garbled.
    Wire(WireError),
    /// A reliable message further ahead than the window allows.
    Window,
    /// A reliable message longer than the cap.
    TooLong,
    /// Signed, and its tag does not pass: not the peer's (or garbled on the way). Dropped unread.
    Forged,
}

impl From<WireError> for ChannelError {
    fn from(e: WireError) -> Self {
        ChannelError::Wire(e)
    }
}

/// Counters of a channel since it was made.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChannelStats {
    pub sent_datagrams: u64,
    pub sent_bytes: u64,
    pub recv_datagrams: u64,
    pub recv_bytes: u64,
    /// Reliable items sent again.
    pub resent: u64,
    /// Datagrams dropped for having been seen already (or being too old to tell).
    pub repeated: u64,
}

#[derive(Clone, Copy, Default)]
struct Sent {
    seq: u16,
    live: bool,
    time: f64,
    carried: Carried,
}

pub struct Channel {
    /// First byte of every datagram of ours: the protocol's mark for "channel data".
    lead: u8,
    seq: u16,
    sent: Vec<Sent>,
    out: Outgoing,
    last_send: f64,
    /// The newest sequence received, the 32 before it as bits, and when it came.
    latest: u16,
    bits: u32,
    latest_at: f64,
    heard: bool,
    last_heard: f64,
    /// A reliable item arrived: the peer waits for our ack.
    ack_due: bool,
    inc: Incoming,
    rtt: Rtt,
    buf: Vec<u8>,
    stats: ChannelStats,
    /// The session's key, once there is one: what every datagram is signed with, both ways.
    key: Option<crate::sip::Key>,
}

impl Channel {
    pub fn new(now: f64, lead: u8) -> Channel {
        // `latest` starts one before the peer's first sequence: nothing is acked by saying it.
        Channel {
            lead,
            seq: 0,
            sent: vec![Sent::default(); SENT],
            out: Outgoing::new(),
            last_send: now,
            latest: u16::MAX,
            bits: 0,
            latest_at: now,
            heard: false,
            last_heard: now,
            ack_due: false,
            inc: Incoming::new(),
            rtt: Rtt::new(),
            buf: vec![0; MTU],
            stats: ChannelStats::default(),
            key: None,
        }
    }
    /// Every datagram signed with `key` from now on, and only those of the peer signed with it
    /// taken.
    pub fn sign(&mut self, key: crate::sip::Key) {
        self.key = Some(key);
    }
    /// Whether `body` (a datagram without its lead byte) is signed with this channel's key (with
    /// no key, anything is).
    pub fn signed(&self, body: &[u8]) -> bool {
        match self.key {
            None => true,
            Some(key) => body.len() >= TAG && {
                let (data, tag) = body.split_at(body.len() - TAG);
                crate::sip::hash(key, data).to_le_bytes() == tag
            },
        }
    }
    /// Queues a message that must arrive, once and in order. False if it is too long or the peer is hopelessly behind.
    pub fn send_reliable(&mut self, data: &[u8]) -> bool {
        self.out.push(data)
    }
    /// Queues a message for the next datagram: it arrives at most once, and not at all if a newer datagram got there first.
    pub fn send_unreliable(&mut self, data: &[u8]) -> bool {
        self.out.push_loose(data)
    }
    /// Sends what is due: the unreliable messages queued, reliable ones never sent or not acked in
    /// time, an ack the peer waits for, or a keep-alive. Nothing due, nothing sent: call it freely.
    pub fn flush(&mut self, now: f64, to: Addr, transport: &mut dyn Transport) {
        if self.out.backlog() == 0 && !self.out.has_loose() && !self.ack_due && now - self.last_send < KEEPALIVE {
            return;
        }
        let resend = self.rtt.resend_after();
        let mut buf = std::mem::take(&mut self.buf);
        let room = if self.key.is_some() { MTU - TAG } else { MTU };
        for _ in 0..MAX_BURST {
            let mut w = Writer::new(&mut buf[..room]);
            w.u8(self.lead);
            w.u16(self.seq);
            w.u16(self.latest);
            w.u32(self.bits);
            // How long the peer's last datagram has waited for this ack, so it can take it off its round-trip time.
            w.u8(if self.heard { ((now - self.latest_at) * 1000.0).clamp(0.0, 255.0) as u8 } else { 255 });
            let (carried, more_reliable) = self.out.write_due(&mut w, now, resend, &mut self.stats);
            let more_loose = self.out.write_loose(&mut w);
            let mut len = w.len();
            if len == HEADER && !self.ack_due && now - self.last_send < KEEPALIVE {
                break;
            }
            if let Some(key) = self.key {
                // (what follows the lead byte, signed)
                let tag = crate::sip::hash(key, &buf[1..len]).to_le_bytes();
                buf[len..len + TAG].copy_from_slice(&tag);
                len += TAG;
            }
            transport.send(to, &buf[..len]);
            self.sent[self.seq as usize % SENT] = Sent { seq: self.seq, live: true, time: now, carried };
            self.seq = self.seq.wrapping_add(1);
            self.last_send = now;
            self.ack_due = false;
            self.stats.sent_datagrams += 1;
            self.stats.sent_bytes += len as u64;
            if !more_reliable && !more_loose {
                break;
            }
        }
        self.buf = buf;
    }
    /// Takes a datagram of the peer (without its lead byte) and leaves its messages in `inbox`.
    pub fn receive(&mut self, body: &[u8], now: f64, inbox: &mut Inbox) -> Result<(), ChannelError> {
        if !self.signed(body) {
            return Err(ChannelError::Forged);
        }
        let body = if self.key.is_some() { &body[..body.len() - TAG] } else { body };
        let mut r = Reader::new(body);
        let (seq, ack, ack_bits, delay) = (r.u16()?, r.u16()?, r.u32()?, r.u8()?);
        self.stats.recv_datagrams += 1;
        self.stats.recv_bytes += body.len() as u64 + 1;
        // Newer than all, or an old one we had not seen, or one to drop.
        let ahead = seq.wrapping_sub(self.latest);
        let stale = if ahead != 0 && ahead < 0x8000 {
            let had = if self.heard { 1u64 << (ahead - 1).min(63) } else { 0 };
            self.bits = if ahead > 32 { 0 } else { ((((self.bits as u64) << ahead) | had) & 0xffff_ffff) as u32 };
            (self.latest, self.latest_at, self.heard) = (seq, now, true);
            false
        } else {
            let back = self.latest.wrapping_sub(seq);
            if back == 0 || back > 32 || self.bits & (1 << (back - 1)) != 0 {
                self.stats.repeated += 1;
                return Ok(());
            }
            self.bits |= 1 << (back - 1);
            true
        };
        self.last_heard = now;
        self.acked(ack, now, Some(delay));
        for i in 0..32 {
            if ack_bits & (1 << i) != 0 {
                self.acked(ack.wrapping_sub(1 + i), now, None);
            }
        }
        while !r.is_empty() {
            let tag = r.u8()?;
            let id = if tag == TAG_LOOSE { 0 } else { r.u16()? };
            let len = r.var()?;
            if len > MTU as u64 {
                return Err(WireError::Long.into());
            }
            let data = r.bytes(len as usize)?;
            match tag {
                TAG_LOOSE if stale => {}
                TAG_LOOSE => inbox.push(false, data),
                TAG_WHOLE | TAG_MORE => {
                    self.ack_due = true;
                    self.inc.take(id, tag == TAG_MORE, data, inbox)?;
                }
                _ => return Err(WireError::Value.into()),
            }
        }
        Ok(())
    }
    fn acked(&mut self, seq: u16, now: f64, delay: Option<u8>) {
        let e = &mut self.sent[seq as usize % SENT];
        if !e.live || e.seq != seq {
            return;
        }
        e.live = false;
        // Only the newest acked datagram says how long its ack waited: only that one measures.
        if let Some(d) = delay.filter(|d| *d < 255) {
            self.rtt.sample((now - e.time) as f32 - d as f32 / 1000.0);
        }
        if e.carried.mask != 0 {
            self.out.acked(e.carried);
        }
    }
    /// Seconds a datagram takes there and back, smoothed (0 until measured).
    pub fn rtt(&self) -> f32 {
        self.rtt.seconds()
    }
    /// Seconds since the peer was last heard (since the channel was made, if never).
    pub fn silence(&self, now: f64) -> f64 {
        now - self.last_heard
    }
    /// Reliable items (messages or pieces) queued and not acked yet.
    pub fn backlog(&self) -> usize {
        self.out.backlog()
    }
    pub fn stats(&self) -> ChannelStats {
        self.stats
    }
}
