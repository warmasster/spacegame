//! What a channel has to send: the reliable messages not acked yet (in order, each resent until
//! it is acked) and the unreliable ones of this moment. Buffers go round: nothing is allocated
//! once the queues have reached their working size.
use super::{ChannelStats, FRAGMENT, MAX_BACKLOG, MAX_LOOSE, MAX_MESSAGE, MAX_UNRELIABLE, TAG_LOOSE, TAG_MORE, TAG_WHOLE, WINDOW};
use crate::wire::Writer;
use std::collections::VecDeque;

struct Item {
    id: u16,
    /// This is a piece of a long message and the next id continues it.
    more: bool,
    acked: bool,
    /// When it last went out; never: minus infinity.
    sent: f64,
    data: Vec<u8>,
}

/// The reliable ids one datagram carried: `first` and, bit by bit, which of the 64 from it.
#[derive(Clone, Copy, Default)]
pub(super) struct Carried {
    pub first: u16,
    pub mask: u64,
}

/// Buffers kept for the next messages (a burst may have needed more: those are let go).
const SPARE: usize = 512;

pub(super) struct Outgoing {
    items: VecDeque<Item>,
    next_id: u16,
    spare: Vec<Vec<u8>>,
    /// Unreliable messages waiting for the next datagram: `[len u16][bytes]` one after another.
    loose: Vec<u8>,
    loose_at: usize,
}

impl Outgoing {
    pub fn new() -> Outgoing {
        Outgoing { items: VecDeque::new(), next_id: 0, spare: Vec::new(), loose: Vec::new(), loose_at: 0 }
    }
    /// Reliable messages (or pieces) not acked yet.
    pub fn backlog(&self) -> usize {
        self.items.len()
    }
    pub fn has_loose(&self) -> bool {
        self.loose_at < self.loose.len()
    }
    /// Queues a reliable message, in pieces if it is long. False (and nothing queued) if it is over the cap or the queue is full.
    pub fn push(&mut self, data: &[u8]) -> bool {
        let pieces = data.len().div_ceil(FRAGMENT).max(1);
        if data.len() > MAX_MESSAGE || self.items.len() + pieces > MAX_BACKLOG {
            return false;
        }
        for k in 0..pieces {
            let mut buf = self.spare.pop().unwrap_or_default();
            buf.clear();
            buf.extend_from_slice(&data[k * FRAGMENT..data.len().min((k + 1) * FRAGMENT)]);
            self.items.push_back(Item { id: self.next_id, more: k + 1 < pieces, acked: false, sent: f64::NEG_INFINITY, data: buf });
            self.next_id = self.next_id.wrapping_add(1);
        }
        true
    }
    /// Queues an unreliable message for the next datagram. False if it cannot fit one or too much is waiting.
    pub fn push_loose(&mut self, data: &[u8]) -> bool {
        if data.len() > MAX_UNRELIABLE || self.loose.len() + data.len() > MAX_LOOSE {
            return false;
        }
        self.loose.extend_from_slice(&(data.len() as u16).to_le_bytes());
        self.loose.extend_from_slice(data);
        true
    }
    /// Writes the reliable items that are due (never sent, or sent `resend` seconds ago without
    /// an ack) while they fit. Returns what was written and whether some were left for another datagram.
    pub fn write_due(&mut self, w: &mut Writer, now: f64, resend: f64, stats: &mut ChannelStats) -> (Carried, bool) {
        let mut carried = Carried::default();
        for item in self.items.iter_mut().take(WINDOW as usize) {
            if item.acked || now - item.sent < resend {
                continue;
            }
            let offset = if carried.mask == 0 { 0 } else { item.id.wrapping_sub(carried.first) };
            if offset > 63 {
                return (carried, true);
            }
            let mark = w.mark();
            w.u8(if item.more { TAG_MORE } else { TAG_WHOLE });
            w.u16(item.id);
            w.var(item.data.len() as u64);
            w.bytes(&item.data);
            if !w.ok() {
                w.rewind(mark);
                return (carried, true);
            }
            if carried.mask == 0 {
                carried.first = item.id;
            }
            carried.mask |= 1 << offset;
            if item.sent.is_finite() {
                stats.resent += 1;
            }
            item.sent = now;
        }
        (carried, false)
    }
    /// Writes the unreliable messages waiting while they fit; true if some were left.
    pub fn write_loose(&mut self, w: &mut Writer) -> bool {
        while self.loose_at < self.loose.len() {
            let len = u16::from_le_bytes([self.loose[self.loose_at], self.loose[self.loose_at + 1]]) as usize;
            let mark = w.mark();
            w.u8(TAG_LOOSE);
            w.var(len as u64);
            w.bytes(&self.loose[self.loose_at + 2..self.loose_at + 2 + len]);
            if !w.ok() {
                w.rewind(mark);
                return true;
            }
            self.loose_at += 2 + len;
        }
        self.loose.clear();
        self.loose_at = 0;
        false
    }
    /// The datagram that carried these arrived: its items need not go again.
    pub fn acked(&mut self, carried: Carried) {
        let Some(front) = self.items.front().map(|i| i.id) else { return };
        let mut mask = carried.mask;
        while mask != 0 {
            let bit = mask.trailing_zeros() as u16;
            mask &= mask - 1;
            let at = carried.first.wrapping_add(bit).wrapping_sub(front) as usize;
            if let Some(item) = self.items.get_mut(at) {
                item.acked = true;
            }
        }
        while self.items.front().is_some_and(|i| i.acked) {
            if let Some(item) = self.items.pop_front()
                && self.spare.len() < SPARE
            {
                self.spare.push(item.data);
            }
        }
    }
}
