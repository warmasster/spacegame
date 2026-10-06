//! A ship — and any structure — as bytes and back: what keeps the copies of one thing in
//! agreement when several games each simulate it (`app::multi`).
//!
//! Nothing here knows a ship by name, nor what a network is: the pieces are
//! - **how it is made** (`write_make`, `make`): the blueprint it is of and, for a piece that came
//!   off one, which of its parts it has and what still joins them: enough for a game that never
//!   had it to have it;
//! - **what is left of it** (`write_state`, `read_state`): each part's hit points and whether it
//!   is there and works, each joint, what its containers hold: the whole of a structure's own
//!   state that is not where it is and how it moves. Only what is not as built is written;
//! - **what changed** (`Shadow::delta`, `read_delta`): the same for the parts and joints that
//!   changed since they were last told: what whoever simulates a structure tells the rest after a
//!   hit, a repair, a clamp let go;
//! - **its systems** (`write_ship`, `read_ship`): the signal store, every control's state, every
//!   machine's and actuator's (`Machine::save`), the joints, the latches, the clamps and what
//!   they hold, the derived signals' memory, the air of every compartment;
//! - **a digest** (`Digest`): a few bytes that two copies compare to know they still agree: a
//!   hash of what must be the same exactly (what is there and works, where the controls are left,
//!   what is latched and held) and a few sums of what need only be near (levels, charges, air,
//!   wear), compared with a tolerance. When it does not match, the copy asks for the rest.
//!
//! Structure ids are each game's own: what names another structure here (what a clamp holds)
//! goes through the caller's `net` / `local` (its id on the wire, its id here).
use crate::ship::Ship;
use glam::{Affine3A, DVec3, Quat, Vec3};
use lunar_controls::intent::F_TRIPPED;
use lunar_core::{
    structure::{
        Library,
        blueprint::Blueprint,
        state::{Joint, NO_ORIGIN, Part, Structure},
    },
};
use lunar_signals::{Quality, SignalId, Writer};
use std::sync::Arc;

/// Why bytes could not be taken.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncError {
    /// They end before what they say does.
    Short,
    /// They are no valid value (a number that is not one, a flag that does not exist).
    Value,
    /// They are of something made otherwise than what they are given to (another ship, another
    /// version of its data): what does not match.
    Count(&'static str),
    /// They name something this game does not have (a blueprint, a part kind).
    Unknown(String),
}

pub type Sync<T> = Result<T, SyncError>;

// ---------------------------------------------------------------------------------------------
// bytes

/// A whole number in as few bytes as it is (7 bits a byte).
pub fn put_var(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let low = (v & 0x7f) as u8;
        v >>= 7;
        out.push(if v == 0 { low } else { low | 0x80 });
        if v == 0 {
            break;
        }
    }
}

pub fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Three floats (12 bytes).
pub fn put_vec3(out: &mut Vec<u8>, v: Vec3) {
    v.to_array().iter().for_each(|x| put_f32(out, *x));
}

/// A point of the world, exact (24 bytes).
pub fn put_dvec3(out: &mut Vec<u8>, v: DVec3) {
    v.to_array().iter().for_each(|x| put_f64(out, *x));
}

/// A turn, exact (16 bytes).
pub fn put_quat(out: &mut Vec<u8>, q: Quat) {
    q.to_array().iter().for_each(|x| put_f32(out, *x));
}

/// Bytes with their length before them.
pub fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_var(out, b.len() as u64);
    out.extend_from_slice(b);
}

pub fn put_str(out: &mut Vec<u8>, s: &str) {
    put_var(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn put_affine(out: &mut Vec<u8>, m: &Affine3A) {
    m.to_cols_array().iter().for_each(|v| put_f32(out, *v));
}

/// A number as few bytes as it is: nothing and one in a byte, what a float holds exactly in
/// five, anything else in nine.
fn put_num(out: &mut Vec<u8>, v: f64) {
    // (a zero of either sign is nothing: nothing reads the sign of a nothing)
    if v == 0.0 {
        out.push(0);
    } else if v == 1.0 {
        out.push(1);
    } else if f64::from(v as f32) == v {
        out.push(2);
        put_f32(out, v as f32);
    } else {
        out.push(3);
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// Numbers one after another, a run of zeros as its length.
fn put_nums(out: &mut Vec<u8>, values: &[f64]) {
    put_var(out, values.len() as u64);
    let mut k = 0;
    while k < values.len() {
        let zeros = values[k..].iter().take_while(|v| **v == 0.0).count();
        if zeros >= 2 {
            out.push(4);
            put_var(out, zeros as u64);
            k += zeros;
        } else {
            put_num(out, values[k]);
            k += 1;
        }
    }
}

/// Reads what the writers above wrote. Never panics: what is short or wrong is an `Err`.
pub struct In<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> In<'a> {
    pub fn new(buf: &'a [u8]) -> In<'a> {
        In { buf, pos: 0 }
    }
    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }
    /// What is left to read.
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos.min(self.buf.len())..]
    }
    pub fn bytes(&mut self, n: usize) -> Sync<&'a [u8]> {
        if n > self.buf.len() - self.pos {
            return Err(SyncError::Short);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Sync<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn var(&mut self) -> Sync<u64> {
        let mut v = 0u64;
        for i in 0..10 {
            let b = self.u8()?;
            if i == 9 && b > 1 {
                return Err(SyncError::Value);
            }
            v |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(SyncError::Value)
    }
    /// A count of things that take at least `each` bytes apiece: one that cannot fit what is left is not ours.
    fn count(&mut self, each: usize) -> Sync<usize> {
        let n = self.var()?;
        if n > ((self.buf.len() - self.pos) / each.max(1)) as u64 + 1 {
            return Err(SyncError::Short);
        }
        Ok(n as usize)
    }
    pub fn f32(&mut self) -> Sync<f32> {
        let b = self.bytes(4)?;
        let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if v.is_finite() { Ok(v) } else { Err(SyncError::Value) }
    }
    pub fn f64(&mut self) -> Sync<f64> {
        let b = self.bytes(8)?;
        let v = f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        if v.is_finite() { Ok(v) } else { Err(SyncError::Value) }
    }
    pub fn vec3(&mut self) -> Sync<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    pub fn dvec3(&mut self) -> Sync<DVec3> {
        Ok(DVec3::new(self.f64()?, self.f64()?, self.f64()?))
    }
    /// A turn: whatever four numbers came, made one (or none at all, if they are no turn).
    pub fn quat(&mut self) -> Sync<Quat> {
        let q = Quat::from_xyzw(self.f32()?, self.f32()?, self.f32()?, self.f32()?);
        if q.length_squared() > 1e-6 { Ok(q.normalize()) } else { Ok(Quat::IDENTITY) }
    }
    /// Bytes `put_bytes` wrote.
    pub fn block(&mut self) -> Sync<&'a [u8]> {
        let n = self.count(1)?;
        self.bytes(n)
    }
    /// A float that may be "not a number" (a store that has not been filled: its mark).
    fn f32_or_nan(&mut self) -> Sync<f32> {
        let b = self.bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn str(&mut self) -> Sync<&'a str> {
        let n = self.count(1)?;
        std::str::from_utf8(self.bytes(n)?).map_err(|_| SyncError::Value)
    }
    fn affine(&mut self) -> Sync<Affine3A> {
        let mut c = [0.0f32; 12];
        for v in &mut c {
            *v = self.f32()?;
        }
        Ok(Affine3A::from_cols_array(&c))
    }
    fn num(&mut self) -> Sync<f64> {
        match self.u8()? {
            0 => Ok(0.0),
            1 => Ok(1.0),
            2 => Ok(f64::from(self.f32()?)),
            3 => {
                let b = self.bytes(8)?;
                let v = f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
                if v.is_finite() { Ok(v) } else { Err(SyncError::Value) }
            }
            _ => Err(SyncError::Value),
        }
    }
    /// Numbers `put_nums` wrote, into `out` (emptied first).
    fn nums(&mut self, out: &mut Vec<f64>) -> Sync<()> {
        let n = self.var()? as usize;
        out.clear();
        while out.len() < n {
            if self.buf.get(self.pos) == Some(&4) {
                self.pos += 1;
                let zeros = self.var()? as usize;
                if zeros == 0 || zeros > n - out.len() {
                    return Err(SyncError::Value);
                }
                out.resize(out.len() + zeros, 0.0);
            } else {
                out.push(self.num()?);
            }
            // (a count that promises more than the bytes can hold ends at the first missing one)
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// what is left of a structure

/// The bits a part is told with.
mod bit {
    pub const ALIVE: u8 = 1;
    pub const WORKING: u8 = 2;
    /// It came off whole and is not missed (`Part::left`).
    pub const LEFT: u8 = 4;
    pub const ANCHOR: u8 = 8;
    /// All its hit points: none follow.
    pub const WHOLE: u8 = 16;
    /// Its store's level follows (`Part::store`).
    pub const STORE: u8 = 32;
    /// As built: there, working, whole.
    pub const PLAIN: u8 = ALIVE | WORKING | WHOLE;
}

fn part_bits(p: &Part) -> u8 {
    let mut b = 0;
    for (on, bit) in [(p.alive, bit::ALIVE), (p.working, bit::WORKING), (p.left, bit::LEFT), (p.anchor, bit::ANCHOR), (p.hp >= p.max_hp, bit::WHOLE)] {
        if on {
            b |= bit;
        }
    }
    b
}

fn write_part(out: &mut Vec<u8>, p: &Part, store: bool) {
    let bits = part_bits(p) | if store && !p.store.is_nan() { bit::STORE } else { 0 };
    out.push(bits);
    if bits & bit::WHOLE == 0 {
        put_f32(out, p.hp);
    }
    if bits & bit::STORE != 0 {
        put_f32(out, p.store);
    }
}

/// Reads a part's state over `p`. True if it was there and no longer is.
fn read_part(inp: &mut In, p: &mut Part) -> Sync<bool> {
    let bits = inp.u8()?;
    if bits & !(bit::PLAIN | bit::LEFT | bit::ANCHOR | bit::STORE) != 0 {
        return Err(SyncError::Value);
    }
    let was = p.alive;
    p.hp = if bits & bit::WHOLE != 0 { p.max_hp } else { inp.f32()?.clamp(0.0, p.max_hp) };
    if bits & bit::STORE != 0 {
        p.store = inp.f32_or_nan()?;
    }
    (p.alive, p.working, p.left, p.anchor) = (bits & bit::ALIVE != 0, bits & bit::WORKING != 0 && !p.fragment, bits & bit::LEFT != 0, bits & bit::ANCHOR != 0);
    Ok(was && !p.alive)
}

/// Everything of structure `s` that is its own and is not where it is and how it moves: what is
/// left of each part and joint, what its containers hold, whether it is anchored. Only what is
/// not as built is written: a structure nothing has happened to is a dozen bytes.
pub fn write_state(s: &Structure, out: &mut Vec<u8>) {
    out.push(u8::from(s.anchored));
    put_var(out, s.parts.len() as u64);
    let told = |p: &Part| part_bits(p) != bit::PLAIN || !p.store.is_nan();
    put_var(out, s.parts.iter().filter(|p| told(p)).count() as u64);
    let mut last = 0;
    for (i, p) in s.parts.iter().enumerate().filter(|(_, p)| told(p)) {
        put_var(out, (i - last) as u64);
        last = i;
        write_part(out, p, true);
    }
    put_var(out, s.joints.len() as u64);
    let told = |j: &Joint| !j.alive || j.hp < j.max_hp;
    put_var(out, s.joints.iter().filter(|j| told(j)).count() as u64);
    let mut last = 0;
    for (k, j) in s.joints.iter().enumerate().filter(|(_, j)| told(j)) {
        put_var(out, (k - last) as u64);
        last = k;
        out.push(u8::from(j.alive));
        put_f32(out, j.hp);
    }
    put_var(out, s.stored.len() as u64);
    for st in &s.stored {
        put_var(out, u64::from(st.part));
        put_f32(out, st.mass);
    }
}

/// Structure `s` left as `write_state` told another copy of it was. The parts that were there
/// and no longer are go into `died` (their place among its parts), for whoever shows them going.
/// `Err(Count)`: it is not the same structure (and nothing was done to it).
pub fn read_state(s: &mut Structure, inp: &mut In, died: &mut Vec<u32>) -> Sync<()> {
    let anchored = inp.u8()? != 0;
    if inp.var()? != s.parts.len() as u64 {
        return Err(SyncError::Count("piezas"));
    }
    let mut next = 0;
    let mut at = 0;
    for n in 0..inp.count(2)? {
        at += inp.var()? as usize;
        if at >= s.parts.len() || (n > 0 && at < next) {
            return Err(SyncError::Value);
        }
        // (the ones between the last told and this one are as built)
        s.parts[next..at].iter_mut().for_each(plain);
        if read_part(inp, &mut s.parts[at])? {
            died.push(at as u32);
        }
        next = at + 1;
    }
    s.parts[next..].iter_mut().for_each(plain);
    if inp.var()? != s.joints.len() as u64 {
        return Err(SyncError::Count("uniones"));
    }
    for j in &mut s.joints {
        (j.alive, j.hp) = (true, j.max_hp);
    }
    let mut at = 0;
    for _ in 0..inp.count(6)? {
        at += inp.var()? as usize;
        let alive = inp.u8()? != 0;
        let hp = inp.f32()?;
        let j = s.joints.get_mut(at).ok_or(SyncError::Value)?;
        (j.alive, j.hp) = (alive, hp.min(j.max_hp));
    }
    for _ in 0..inp.count(5)? {
        let (part, kg) = (inp.var()? as u32, inp.f32()?);
        s.fill(part, kg);
    }
    s.anchored = anchored;
    s.refresh();
    Ok(())
}

/// Part `p` as built (it was not told of: nothing has happened to it).
fn plain(p: &mut Part) {
    (p.alive, p.working, p.left, p.anchor, p.hp, p.store) = (true, !p.fragment, false, false, p.max_hp, f32::NAN);
}

// ---------------------------------------------------------------------------------------------
// what changed

/// What was last told of each part and joint of a structure, to tell only what changed since.
#[derive(Clone, Debug, Default)]
pub struct Shadow {
    parts: Vec<(f32, u8)>,
    joints: Vec<(f32, bool)>,
}

/// A part's hit points are told again when they are this share of all it has from what was last told.
const HP_STEP: f32 = 1.0 / 128.0;

impl Shadow {
    /// As structure `s` is now: nothing of it is to be told until it changes.
    pub fn of(s: &Structure) -> Shadow {
        let mut sh = Shadow::default();
        sh.take(s);
        sh
    }

    /// Nothing known: everything of whatever it is compared with is to be told.
    pub fn unknown() -> Shadow {
        Shadow::default()
    }

    /// Takes structure `s` as it is now for told (keeping its buffers).
    pub fn take(&mut self, s: &Structure) {
        self.parts.clear();
        self.parts.extend(s.parts.iter().map(|p| (p.hp, part_bits(p))));
        self.joints.clear();
        self.joints.extend(s.joints.iter().map(|j| (j.hp, j.alive)));
    }

    /// What of structure `s` changed since it was last told (the parts that are or are not there,
    /// work or do not, or have lost or gained hit points worth telling; the joints that gave or
    /// were made again), written to `out` for `read_delta`, and taken for told. False (and
    /// nothing written) if nothing did.
    pub fn delta(&mut self, s: &Structure, out: &mut Vec<u8>) -> bool {
        // (another structure altogether: a piece just come off, a ship rebuilt: all of it)
        if self.parts.len() != s.parts.len() || self.joints.len() != s.joints.len() {
            self.parts.clear();
            self.parts.resize(s.parts.len(), (f32::NAN, 0xff));
            self.joints.clear();
            self.joints.resize(s.joints.len(), (f32::NAN, true));
        }
        // (whether it is whole is in its hit points: a scratch is not news by itself)
        let part = |p: &Part, was: &(f32, u8)| (part_bits(p) ^ was.1) & !bit::WHOLE != 0 || !((p.hp - was.0).abs() < p.max_hp * HP_STEP);
        let joint = |j: &Joint, was: &(f32, bool)| j.alive != was.1 || !((j.hp - was.0).abs() < j.max_hp * HP_STEP);
        let parts = s.parts.iter().zip(&self.parts).filter(|(p, was)| part(p, was)).count();
        let joints = s.joints.iter().zip(&self.joints).filter(|(j, was)| joint(j, was)).count();
        if parts == 0 && joints == 0 {
            return false;
        }
        put_var(out, s.parts.len() as u64);
        put_var(out, parts as u64);
        for (i, (p, was)) in s.parts.iter().zip(&mut self.parts).enumerate() {
            if part(p, was) {
                put_var(out, i as u64);
                write_part(out, p, false);
                *was = (p.hp, part_bits(p));
            }
        }
        put_var(out, joints as u64);
        for (k, (j, was)) in s.joints.iter().zip(&mut self.joints).enumerate() {
            if joint(j, was) {
                put_var(out, k as u64);
                out.push(u8::from(j.alive));
                put_f32(out, j.hp);
                *was = (j.hp, j.alive);
            }
        }
        true
    }
}

/// What `Shadow::delta` told of another copy of structure `s`, done to it. The parts that were
/// there and no longer are go into `died`.
pub fn read_delta(s: &mut Structure, inp: &mut In, died: &mut Vec<u32>) -> Sync<()> {
    if inp.var()? != s.parts.len() as u64 {
        return Err(SyncError::Count("piezas"));
    }
    for _ in 0..inp.count(2)? {
        let at = inp.var()? as usize;
        let p = s.parts.get_mut(at).ok_or(SyncError::Value)?;
        if read_part(inp, p)? {
            died.push(at as u32);
        }
    }
    for _ in 0..inp.count(6)? {
        let at = inp.var()? as usize;
        let alive = inp.u8()? != 0;
        let hp = inp.f32()?;
        let j = s.joints.get_mut(at).ok_or(SyncError::Value)?;
        (j.alive, j.hp) = (alive, hp.min(j.max_hp));
    }
    // (what a part that is gone held is gone with it)
    s.refresh();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// how it is made

/// The blueprint structure `s` is of (or is a piece of): the one of its name whose parts are its.
pub fn blueprint_of<'a>(s: &Structure, lib: &'a Library) -> Option<&'a (String, Blueprint)> {
    lib.blueprints.iter().find(|(_, bp)| *bp.name == *s.name && s.parts.iter().all(|p| p.origin != NO_ORIGIN && bp.parts.get(p.origin as usize).is_some_and(|b| b.kind == p.kind)))
}

/// Whether `s` is its whole blueprint, part for part (nothing came off it into a structure of its own... or it is what was left).
fn whole(s: &Structure, bp: &Blueprint) -> bool {
    s.parts.len() == bp.parts.len() && s.joints.len() == bp.joints.len() && s.parts.iter().enumerate().all(|(i, p)| p.origin == i as u32)
}

/// How to make structure `s` in a game that does not have it: its blueprint and, if it is a piece
/// that came off one, which parts of it and what joins them. `Err(Unknown)`: it is of no
/// blueprint (pieces of broken parts: they have shapes of their own, which do not travel).
pub fn write_make(s: &Structure, lib: &Library, out: &mut Vec<u8>) -> Sync<()> {
    let (id, bp) = blueprint_of(s, lib).ok_or_else(|| SyncError::Unknown(s.name.to_string()))?;
    if s.parts.iter().any(|p| p.fragment || !Arc::ptr_eq(&p.shape, &lib.catalog.parts[usize::from(p.kind)].shape)) {
        return Err(SyncError::Unknown(format!("{}: piezas con forma propia", s.name)));
    }
    if whole(s, bp) {
        out.push(0);
        put_str(out, id);
        return Ok(());
    }
    out.push(1);
    put_str(out, id);
    put_var(out, s.parts.len() as u64);
    for p in &s.parts {
        put_var(out, u64::from(p.origin));
        // (where it was when it came off, if a joint had moved it from where it is built)
        let moved = p.local != bp.parts[p.origin as usize].local;
        out.push(u8::from(moved));
        if moved {
            put_affine(out, &p.local);
        }
    }
    put_var(out, s.joints.len() as u64);
    for j in &s.joints {
        put_var(out, u64::from(j.a));
        put_var(out, u64::from(j.b));
        put_var(out, u64::from(j.kind));
        j.at.to_array().iter().for_each(|v| put_f32(out, *v));
        put_f32(out, j.max_hp);
        put_var(out, u64::from(j.networks));
    }
    // (a big piece keeps its blueprint's look, posed as it was when it came off)
    out.push(u8::from(s.model));
    if s.model {
        put_var(out, s.bones.len() as u64);
        s.bones.iter().for_each(|b| put_affine(out, b));
    }
    Ok(())
}

/// Whether `bytes` (what `write_make` wrote) say how structure `s` is made: the same blueprint,
/// the same parts of it. What is made the same is told its state; what is not is made again.
pub fn made_as(s: &Structure, lib: &Library, bytes: &[u8]) -> bool {
    let mut mine = Vec::new();
    // (pieces: the same parts where they were, joined the same: byte for byte what we would say)
    write_make(s, lib, &mut mine).is_ok() && mine == bytes
}

/// The structure `write_make` told of, made here with id `id`, at rest at `pos` turned `rot`:
/// every part whole and there (`read_state` then says what is left of it).
pub fn make(lib: &Library, id: u64, pos: DVec3, rot: Quat, inp: &mut In) -> Sync<Structure> {
    let form = inp.u8()?;
    let name = inp.str()?;
    let bp = lib.blueprint(name).ok_or_else(|| SyncError::Unknown(name.to_string()))?;
    let cat = &lib.catalog;
    match form {
        0 => {
            let mut s = Structure::new(id, bp, cat, pos, rot);
            // (as `Structures::spawn` leaves it: whoever tells its state says if it is anchored and by what)
            s.anchored = false;
            Ok(s)
        }
        1 => {
            let mut parts = Vec::new();
            for _ in 0..inp.count(2)? {
                let origin = inp.var()? as usize;
                let placed = bp.parts.get(origin).ok_or(SyncError::Value)?;
                let local = if inp.u8()? != 0 { inp.affine()? } else { placed.local };
                let mut p = Part::new(cat, placed.kind, local, None, false);
                p.origin = origin as u32;
                parts.push(p);
            }
            let mut joints = Vec::new();
            for _ in 0..inp.count(20)? {
                let (a, b, kind) = (inp.var()? as u32, inp.var()? as u32, inp.var()?);
                let at = Vec3::new(inp.f32()?, inp.f32()?, inp.f32()?);
                let max_hp = inp.f32()?;
                let networks = inp.var()? as u32;
                if a as usize >= parts.len() || b as usize >= parts.len() || kind >= cat.joints.len() as u64 {
                    return Err(SyncError::Value);
                }
                joints.push(Joint { a, b, kind: kind as u16, at, hp: max_hp, max_hp, networks, alive: true });
            }
            let model = inp.u8()? != 0;
            let mut s = Structure::assemble_holding(id, bp.name.as_str().into(), pos, rot, false, parts, joints, cat);
            if model {
                let n = inp.count(48)?;
                s.bones = (0..n).map(|_| inp.affine()).collect::<Sync<Vec<Affine3A>>>()?;
                s.model = true;
            }
            Ok(s)
        }
        _ => Err(SyncError::Value),
    }
}

// ---------------------------------------------------------------------------------------------
// a ship's systems

/// Everything a ship's systems are at this moment, for another copy of the same kind of ship to
/// be left the same (`read_ship`): its signals, its controls, its machines and actuators, its
/// joints, latches and clamps, the memory of its derived signals, its air. `net`: the id on the
/// wire of a structure of this game (what a clamp holds); what has none is left out.
pub fn write_ship(sh: &Ship, net: &dyn Fn(u64) -> Option<u64>, out: &mut Vec<u8>) {
    let mut nums: Vec<f64> = Vec::new();
    // signals: every value, and which cannot be trusted
    put_nums(out, sh.store.values());
    let bad: Vec<(usize, u8)> = sh.store.qualities().iter().enumerate().filter(|(_, q)| **q != Quality::Ok).map(|(i, q)| (i, if *q == Quality::Stale { 1 } else { 2 })).collect();
    put_var(out, bad.len() as u64);
    for (i, q) in bad {
        put_var(out, i as u64);
        out.push(q);
    }
    // controls
    put_var(out, sh.panels.controls.len() as u64);
    for c in &sh.panels.controls {
        [c.st.x, c.st.y, c.st.v, c.st.t].iter().for_each(|v| put_num(out, *v));
        put_var(out, u64::from(c.st.flags));
    }
    // machines, actuators
    put_var(out, sh.machines.len() as u64);
    for m in &sh.machines {
        out.push(u8::from(m.working));
        nums.clear();
        m.m.save(&mut nums);
        put_nums(out, &nums);
    }
    put_var(out, sh.actuators.len() as u64);
    for a in &sh.actuators {
        nums.clear();
        a.save(&mut nums);
        put_nums(out, &nums);
    }
    // joints, latches, clamps
    put_var(out, sh.joints.len() as u64);
    for j in &sh.joints {
        put_num(out, j.q);
        put_num(out, j.qd);
    }
    put_var(out, sh.latched.len() as u64);
    sh.latched.iter().for_each(|l| out.push(u8::from(*l)));
    put_var(out, sh.clamp_held.len() as u64);
    for (c, held) in sh.clamp_held.iter().enumerate() {
        out.push(u8::from(sh.clamp_open.get(c).copied().unwrap_or(false)));
        let ids: Vec<u64> = held.iter().filter_map(|id| net(*id)).collect();
        put_var(out, ids.len() as u64);
        ids.iter().for_each(|id| put_var(out, *id));
    }
    put_nums(out, sh.derived.state());
    // air
    put_var(out, sh.atmos.air.len() as u64);
    for a in &sh.atmos.air {
        [a.o2, a.n2, a.co2, a.t].iter().for_each(|v| put_num(out, *v));
    }
}

/// Ship `sh` (on its structure `s`, whose own state was read already: `read_state`) left as
/// `write_ship` told another copy of it was. `local`: the id here of a structure by its id on the
/// wire. `Err(Count)`: it is not the same kind of ship; what was read before the mismatch has
/// been applied (a ship of other data cannot be told this way at all).
pub fn read_ship(sh: &mut Ship, s: &mut Structure, local: &dyn Fn(u64) -> Option<u64>, inp: &mut In) -> Sync<()> {
    let mut nums: Vec<f64> = Vec::new();
    inp.nums(&mut nums)?;
    if nums.len() != sh.store.len() {
        return Err(SyncError::Count("señales"));
    }
    sh.store.load(&nums);
    for _ in 0..inp.count(2)? {
        let (i, q) = (inp.var()?, inp.u8()?);
        if i >= sh.store.len() as u64 {
            return Err(SyncError::Value);
        }
        sh.store.set_quality(SignalId(i as u32), if q == 1 { Quality::Stale } else { Quality::Failed });
    }
    if inp.var()? != sh.panels.controls.len() as u64 {
        return Err(SyncError::Count("mandos"));
    }
    for c in &mut sh.panels.controls {
        (c.st.x, c.st.y, c.st.v, c.st.t) = (inp.num()?, inp.num()?, inp.num()?, inp.num()?);
        c.st.flags = inp.var()? as u32;
        c.held = 0.0;
    }
    if inp.var()? != sh.machines.len() as u64 {
        return Err(SyncError::Count("máquinas"));
    }
    for m in &mut sh.machines {
        m.working = inp.u8()? != 0;
        inp.nums(&mut nums)?;
        m.m.load(&nums);
        // (its part as the structure has it now: nothing of it is taken for just destroyed)
        m.was_alive = m.part.is_none_or(|p| s.parts[p as usize].alive);
    }
    if inp.var()? != sh.actuators.len() as u64 {
        return Err(SyncError::Count("actuadores"));
    }
    for a in &mut sh.actuators {
        inp.nums(&mut nums)?;
        a.load(&nums);
    }
    if inp.var()? != sh.joints.len() as u64 {
        return Err(SyncError::Count("articulaciones"));
    }
    for j in &mut sh.joints {
        (j.q, j.qd) = (inp.num()?.clamp(j.lo, j.hi), inp.num()?);
    }
    sh.update_poses();
    s.set_pose(&sh.poses);
    if inp.var()? != sh.latched.len() as u64 {
        return Err(SyncError::Count("cierres"));
    }
    for l in &mut sh.latched {
        *l = inp.u8()? != 0;
    }
    if inp.var()? != sh.clamp_held.len() as u64 {
        return Err(SyncError::Count("anclajes"));
    }
    for c in 0..sh.clamp_held.len() {
        let open = inp.u8()? != 0;
        if let Some(o) = sh.clamp_open.get_mut(c) {
            *o = open;
        }
        sh.clamp_held[c].clear();
        for _ in 0..inp.count(1)? {
            if let Some(id) = local(inp.var()?) {
                sh.clamp_held[c].push(id);
            }
        }
    }
    inp.nums(&mut nums)?;
    sh.derived.load_state(&nums);
    if inp.var()? != sh.atmos.air.len() as u64 {
        return Err(SyncError::Count("compartimentos"));
    }
    for a in &mut sh.atmos.air {
        (a.o2, a.n2, a.co2, a.t) = (inp.num()?, inp.num()?, inp.num()?, inp.num()?);
    }
    sh.atmos.settled();
    sh.touch();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// a digest

/// Sums of what need only be near, by turns.
pub const LEVELS: usize = 8;

/// What two copies of a structure (and of its ship, if it is one) compare to know they agree.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Digest {
    /// Of what must be the same exactly: which parts are there and work, which joints hold,
    /// where each control is left and which breakers are tripped, what is latched, which clamps
    /// are open and how much each holds, the signals nobody but a hand writes.
    pub hash: u32,
    /// Of what need only be near: what the machines keep (levels, charges, heat, wear), the air,
    /// what the containers hold, the hit points left.
    pub levels: [f32; LEVELS],
}

/// Bytes of a digest on the wire.
pub const DIGEST_BYTES: usize = 4 + 4 * LEVELS;
/// How far two copies' sums may be from each other and still agree (`Digest::near`).
pub const NEAR: f32 = 0.25;
pub const NEAR_HP: f32 = 0.3;

struct Fnv(u32);

impl Fnv {
    fn byte(&mut self, b: u8) {
        self.0 = (self.0 ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    fn word(&mut self, w: i64) {
        w.to_le_bytes().iter().for_each(|b| self.byte(*b));
    }
}

/// A value as it counts in a sum: by its size, not by its unit (a pressure in pascals and a
/// level in kilograms weigh alike).
fn level(v: f64) -> f32 {
    if v.is_finite() { (v.signum() * v.abs().ln_1p()) as f32 } else { 0.0 }
}

impl Digest {
    pub fn of(sh: Option<&Ship>, s: &Structure) -> Digest {
        let mut h = Fnv(0x811c_9dc5);
        let mut levels = [0.0f32; LEVELS];
        let mut hp = 0.0f64;
        for p in &s.parts {
            h.byte(u8::from(p.alive) | u8::from(p.working) << 1 | u8::from(p.left) << 2);
            if p.alive && p.max_hp > 0.0 {
                hp += f64::from(p.hp / p.max_hp);
            }
        }
        levels[LEVELS - 1] = hp as f32;
        s.joints.iter().for_each(|j| h.byte(u8::from(j.alive)));
        h.byte(u8::from(s.anchored));
        let mut k = 0;
        let mut add = |v: f64| {
            levels[k % (LEVELS - 1)] += level(v);
            k += 1;
        };
        s.stored.iter().for_each(|st| add(f64::from(st.mass)));
        if let Some(sh) = sh {
            for c in &sh.panels.controls {
                h.word((c.mech.value(&c.st) * 1024.0).round() as i64);
                h.byte(u8::from(c.st.flags & F_TRIPPED != 0));
            }
            sh.latched.iter().for_each(|l| h.byte(u8::from(*l)));
            for (c, held) in sh.clamp_held.iter().enumerate() {
                h.byte(u8::from(sh.clamp_open.get(c).copied().unwrap_or(false)));
                h.byte(held.len() as u8);
            }
            // (what only a hand writes: an order given at a door, a signal the data starts with)
            for id in sh.store.ids().filter(|id| sh.store.meta(*id).writer == Writer::None) {
                h.word((sh.store.get(id) * 1024.0).round() as i64);
            }
            let mut nums = Vec::new();
            for m in &sh.machines {
                h.byte(u8::from(m.working));
                nums.clear();
                m.m.save(&mut nums);
                nums.iter().for_each(|v| add(*v));
            }
            for a in &sh.atmos.air {
                [a.o2, a.n2, a.co2, a.t].iter().for_each(|v| add(*v));
            }
        }
        Digest { hash: h.0, levels }
    }

    /// Whether another copy's digest says the same: what must be the same is, and what need only
    /// be near is.
    pub fn agrees(&self, other: &Digest) -> bool {
        self.hash == other.hash && self.near(other)
    }

    /// What need only be near is: each sum within `NEAR` of the other's — which is one of the
    /// things it adds up off by a quarter, or two dozen of them off by a hundredth each (values
    /// count by their size, `level`); and the hit points left within `NEAR_HP` of a part's.
    pub fn near(&self, other: &Digest) -> bool {
        self.levels.iter().zip(&other.levels).enumerate().all(|(k, (a, b))| (a - b).abs() <= if k == LEVELS - 1 { NEAR_HP } else { NEAR })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.hash.to_le_bytes());
        self.levels.iter().for_each(|v| put_f32(out, *v));
    }

    pub fn read(inp: &mut In) -> Sync<Digest> {
        let b = inp.bytes(4)?;
        let mut d = Digest { hash: u32::from_le_bytes([b[0], b[1], b[2], b[3]]), levels: [0.0; LEVELS] };
        for v in &mut d.levels {
            *v = inp.f32()?;
        }
        Ok(d)
    }
}
