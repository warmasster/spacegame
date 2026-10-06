//! Batches of states: the body of `Msg::Up` (a client's own record and the things it holds, all
//! of one moment) and of `Msg::Down` (what the server passes on to a client: states of several
//! senders, each with the moment its sender sampled it). Moments are microseconds of the server's
//! clock. What a state says is the game's business: here it is a run of bytes with a name.
//!
//! Up:   `[stamp]` then entries `[head][key, if a thing's][length][bytes]`.
//! Down: `[base]` then entries `[head][age, if STAMPED][player, if a player's own][key, if a
//! thing's][length][bytes]`: the moment of an entry is `base - age` (`age` is signed: a sender's
//! idea of the server's clock may run a few milliseconds ahead); an entry without `STAMPED` has
//! the moment of the one before it (the states of one sender come together and share theirs).
//!
//! A state is either its sender's own (`Whose::Own`: their player) or of a thing they hold the key
//! of (`Whose::Thing`), and one of `SUBS` streams of it (a ship's hull, its joints: they change
//! at different times and go apart). The other bits of `head`: `HELD` (the thing did not change
//! from its sender's previous send until one tick before this one: see `throttle`) and `STAMPED`.
use super::tag;
use crate::wire::{Reader, Wire, WireError, Writer};

/// Streams a thing (or a player) may have.
pub const SUBS: usize = 4;
/// The longest state: more is not ours.
pub const MAX_STATE: usize = 1024;

/// Whose state an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whose {
    /// Its sender's own.
    Own,
    /// Of the thing with this key (`game::key`), which its sender holds.
    Thing(u64),
}

const THING: u8 = 1;
const SUB: u8 = 6;
const HELD: u8 = 8;
const STAMPED: u8 = 16;

fn head(whose: Whose, sub: u8, held: bool) -> u8 {
    u8::from(matches!(whose, Whose::Thing(_))) | ((sub << 1) & SUB) | if held { HELD } else { 0 }
}

/// Starts an `Up` message (its tag and the moment of its states).
pub fn begin_up(w: &mut Writer, stamp: u64) {
    w.u8(tag::UP);
    w.var(stamp);
}

/// Adds a state, already encoded, to an `Up` message. False (and nothing written) if it does not fit.
pub fn up_entry(w: &mut Writer, whose: Whose, sub: u8, held: bool, raw: &[u8]) -> bool {
    let mark = w.mark();
    w.u8(head(whose, sub, held));
    if let Whose::Thing(key) = whose {
        w.var(key);
    }
    w.var(raw.len() as u64);
    w.bytes(raw);
    if !w.ok() {
        w.rewind(mark);
        return false;
    }
    true
}

fn read_raw<'a>(r: &mut Reader<'a>) -> Wire<&'a [u8]> {
    let n = r.var()?;
    if n > MAX_STATE as u64 {
        return Err(WireError::Long);
    }
    r.bytes(n as usize)
}

/// An entry of an `Up` batch.
#[derive(Clone, Copy, Debug)]
pub struct UpEntry<'a> {
    pub whose: Whose,
    pub sub: u8,
    pub held: bool,
    /// The state's bytes as they came.
    pub raw: &'a [u8],
}

pub struct UpReader<'a> {
    r: Reader<'a>,
    stamp: u64,
}

impl<'a> UpReader<'a> {
    pub fn new(body: &'a [u8]) -> Wire<UpReader<'a>> {
        let mut r = Reader::new(body);
        let stamp = r.var()?;
        Ok(UpReader { r, stamp })
    }
    pub fn stamp(&self) -> u64 {
        self.stamp
    }
    pub fn next(&mut self) -> Wire<Option<UpEntry<'a>>> {
        if self.r.is_empty() {
            return Ok(None);
        }
        let head = self.r.u8()?;
        if head & !(THING | SUB | HELD) != 0 {
            return Err(WireError::Value);
        }
        let whose = if head & THING != 0 { Whose::Thing(self.r.var()?) } else { Whose::Own };
        Ok(Some(UpEntry { whose, sub: (head & SUB) >> 1, held: head & HELD != 0, raw: read_raw(&mut self.r)? }))
    }
}

/// Writes the entries of a `Down` message.
pub struct DownWriter {
    base: u64,
    last: u64,
}

impl DownWriter {
    /// Starts a `Down` message; `base` is the server's clock now.
    pub fn begin(w: &mut Writer, base: u64) -> DownWriter {
        w.u8(tag::DOWN);
        w.var(base);
        DownWriter { base, last: base }
    }
    /// Adds a state already encoded; `player`: whose, for a player's own (a thing's key is in `whose`).
    /// False (and nothing written) if it does not fit.
    #[allow(clippy::too_many_arguments)]
    pub fn entry(&mut self, w: &mut Writer, whose: Whose, sub: u8, player: u32, stamp: u64, held: bool, raw: &[u8]) -> bool {
        let mark = w.mark();
        let head = head(whose, sub, held);
        if stamp == self.last {
            w.u8(head);
        } else {
            w.u8(head | STAMPED);
            w.zig(self.base as i64 - stamp as i64);
        }
        match whose {
            Whose::Own => w.var(player as u64),
            Whose::Thing(key) => w.var(key),
        }
        w.var(raw.len() as u64);
        w.bytes(raw);
        if !w.ok() {
            w.rewind(mark);
            return false;
        }
        self.last = stamp;
        true
    }
}

/// What an entry of a `Down` batch is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Player `0`'s own record.
    Player(u32),
    /// The thing with this key.
    Thing(u64),
}

/// An entry of a `Down` batch.
#[derive(Clone, Copy, Debug)]
pub struct DownEntry<'a> {
    pub source: Source,
    pub sub: u8,
    pub stamp: u64,
    pub held: bool,
    pub raw: &'a [u8],
}

pub struct DownReader<'a> {
    r: Reader<'a>,
    base: u64,
    last: u64,
}

impl<'a> DownReader<'a> {
    pub fn new(body: &'a [u8]) -> Wire<DownReader<'a>> {
        let mut r = Reader::new(body);
        let base = r.var()?;
        Ok(DownReader { r, base, last: base })
    }
    /// The server's clock when it sent the batch.
    pub fn base(&self) -> u64 {
        self.base
    }
    pub fn next(&mut self) -> Wire<Option<DownEntry<'a>>> {
        if self.r.is_empty() {
            return Ok(None);
        }
        let head = self.r.u8()?;
        if head & !(THING | SUB | HELD | STAMPED) != 0 {
            return Err(WireError::Value);
        }
        if head & STAMPED != 0 {
            self.last = (self.base as i64).saturating_sub(self.r.zig()?).max(0) as u64;
        }
        let source = if head & THING != 0 { Source::Thing(self.r.var()?) } else { Source::Player(self.r.var32()?) };
        Ok(Some(DownEntry { source, sub: (head & SUB) >> 1, stamp: self.last, held: head & HELD != 0, raw: read_raw(&mut self.r)? }))
    }
}
